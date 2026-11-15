// Checks Tab Swarm end to end, and the realm's refusals one by one.
// Run ../build.sh first. Usage: node test/smoke.mjs
import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";
import { createIdentity, createRealm } from "../../../web/kernel/realm.js";

const run = promisify(execFile);
const cliPath = fileURLToPath(new URL("../local/realm.mjs", import.meta.url));
let failed = 0;
function check(label, ok, detail = "") {
  if (!ok) failed++;
  console.log(`${ok ? "PASS" : "FAIL"} ${label}${detail ? "  " + detail : ""}`);
}

// ---- the whole example, over real WebSockets, with workers in this machine ----
async function swarm(...args) {
  const { stdout } = await run("node", [cliPath, "--port", "8793", "--json", ...args]);
  return { stdout, json: JSON.parse(stdout.trim().split("\n").pop()) };
}
const PRIMES_BELOW_5M = 348513;

let r = await swarm("--local-workers", "3", "--wait", "3", "--message", "primes 5000000");
check("three realms count the primes below 5,000,000", r.json.total === PRIMES_BELOW_5M, `total=${r.json.total}`);
check("one intent per piece with dispatch all", r.json.intents === 6, `intents=${r.json.intents}`);

r = await swarm("--local-workers", "4", "--faulty", "1", "--wait", "4", "--message", "primes 5000000");
check("without quorum a faulty realm corrupts the total", r.json.total !== PRIMES_BELOW_5M, `total=${r.json.total}`);

r = await swarm("--local-workers", "4", "--faulty", "1", "--wait", "4", "--mode", "quorum", "--message", "primes 5000000");
check("with quorum the total is right", r.json.total === PRIMES_BELOW_5M, `total=${r.json.total}`);
check("and the faulty realm is named", Object.keys(r.json.dissent).join() === "node-1", JSON.stringify(r.json.dissent));

r = await swarm("--local-workers", "3", "--wait", "3", "--mode", "first", "--message", "primes 1000000");
check("dispatch first asks every member for every piece", r.json.intents === 18 && r.json.total === 78498);

// ---- refusals, with two realms joined by an in-memory wire ----
async function pair({ leadPolicy, workerPolicy, tamper = (m) => m, delayMs = 0 }) {
  const wires = {};
  const ledgers = { lead: [], worker: [] };
  const make = async (name, labels, policy, serve) => {
    const identity = await createIdentity(name, labels);
    const realm = createRealm({
      identity, policy, serve,
      send: (to, message) => setTimeout(() => wires[to]?.receive(tamper(structuredClone(message), to)), delayMs),
      onLedger: (e) => ledgers[name].push(e),
    });
    wires[name] = realm;
    return realm;
  };
  const lead = await make("lead", { role: "lead" }, leadPolicy, {});
  const worker = await make("worker", { role: "worker" }, workerPolicy, { "primes.count": () => ({ count: 4 }), "files.read": () => "secret" });
  lead.setPeers([worker.advert()]);
  worker.setPeers([lead.advert()]);
  return { lead, worker, ledgers, wires };
}
const open = { egress: [{ to: {}, verbs: ["*"] }], ingress: [] };
const strict = { ingress: [{ from: { name: "lead" }, verbs: ["primes.count"] }], egress: [] };
const denial = (ledger) => ledger.find((e) => e.decision === "deny")?.by;
const fails = (promise) => promise.then(() => null, (e) => e.message);

let p = await pair({ leadPolicy: open, workerPolicy: strict });
check("an allowed intent is served", (await p.lead.ask("worker", "primes.count", {})).count === 4);
check("a verb outside ingress is refused by the receiver", /policy:ingress/.test(await fails(p.lead.ask("worker", "files.read", {}))));
check("and the refusal is in the receiver's ledger", denial(p.ledgers.worker) === "policy:ingress");

p = await pair({ leadPolicy: { egress: [], ingress: [] }, workerPolicy: strict });
check("with no egress rule the intent never leaves", /policy does not allow/.test(await fails(p.lead.ask("worker", "primes.count", {}))));
check("so the receiver saw nothing", p.ledgers.worker.length === 0);

p = await pair({ leadPolicy: open, workerPolicy: strict, tamper: (m) => { if (m.intent) m.intent.body = { from: 0, to: 9 }; return m; } });
check("an intent changed on the wire fails its signature", /bad signature/.test(await fails(p.lead.ask("worker", "primes.count", {}))));

p = await pair({ leadPolicy: open, workerPolicy: strict, tamper: (m) => { if (m.result) m.result.value = { count: 999 }; return m; } });
check("a result changed on the wire is dropped", /did not answer/.test(await fails(p.lead.ask("worker", "primes.count", {}, { timeoutMs: 300 }))));

p = await pair({ leadPolicy: open, workerPolicy: strict });
let captured;
const original = p.wires.worker.receive;
p.wires.worker.receive = (m) => { captured ??= structuredClone(m); return original(m); };
await p.lead.ask("worker", "primes.count", {});
await original(captured);
check("the same intent sent twice is refused as a replay", denial(p.ledgers.worker) === "replayed");

p = await pair({ leadPolicy: open, workerPolicy: strict, delayMs: 60 });
await fails(p.lead.ask("worker", "primes.count", {}, { timeoutMs: 20 }));
await new Promise((resolve) => setTimeout(resolve, 100));
check("an intent that arrives after it expired is refused", denial(p.ledgers.worker) === "expired", String(denial(p.ledgers.worker)));

p = await pair({ leadPolicy: open, workerPolicy: strict });
const stranger = createRealm({ identity: await createIdentity("mallory", {}), policy: open, send: (to, m) => p.wires[to].receive(m) });
stranger.setPeers([p.worker.advert()]);
check("a realm the receiver does not know is refused", /did not answer|unknown realm/.test(await fails(stranger.ask("worker", "primes.count", {}, { timeoutMs: 300 }))));
check("and that is in the ledger too", denial(p.ledgers.worker) === "unknown realm");

p = await pair({ leadPolicy: open, workerPolicy: strict });
check("a pool larger than maxFanout is refused before anything is sent",
  /maxFanout/.test(await fails(p.lead.askPool({ labels: { role: "worker" } }, "primes.count", [{}, {}, {}], { maxFanout: 2 }))) && p.ledgers.worker.length === 0);

p = await pair({ leadPolicy: open, workerPolicy: strict, delayMs: 300 });
let waiting = fails(p.lead.ask("worker", "primes.count", {}));
await new Promise((resolve) => setTimeout(resolve, 30));   // the intent is on its way
let asked = Date.now();
p.lead.setPeers([]);
check("when a realm leaves, what was waiting on it fails at once", /worker left/.test(await waiting) && Date.now() - asked < 100);

p = await pair({ leadPolicy: open, workerPolicy: strict, delayMs: 300 });
waiting = fails(p.lead.ask("worker", "primes.count", {}));
asked = Date.now();
p.lead.setPeers([]);                                       // it leaves before the intent is even signed
check("also when it leaves while the intent is being signed", /worker left/.test(await waiting) && Date.now() - asked < 100);

// ---- the relay refuses a realm with the wrong room token ----
{
  const { spawn } = await import("node:child_process");
  const { startTabRealm } = await import("../web/src/tab-realm.js");
  const child = spawn("node", [cliPath, "--port", "8794", "--token", "right"], { stdio: ["pipe", "ignore", "ignore"] });
  await new Promise((resolve) => setTimeout(resolve, 800));
  const status = await new Promise((resolve) => {
    startTabRealm({ url: "ws://localhost:8794/relay", token: "wrong", name: "intruder", worker: { invoke: () => "{}" },
      onStatus: (text) => text !== "disconnected" && resolve(text) });
    setTimeout(() => resolve("no answer"), 2000);
  });
  check("the relay refuses a realm with the wrong token", status === "refused: wrong token", status);
  child.kill();
}

console.log(`\n${failed ? failed + " FAILED" : "all checks passed"}`);
process.exit(failed ? 1 : 0);
