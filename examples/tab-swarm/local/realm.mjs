#!/usr/bin/env node
// The local realm of Tab Swarm: a lead agent (a WASM component), the relay
// that browser realms join, and a prompt. Run ../build.sh first.
import { readFile } from "node:fs/promises";
import { createServer } from "node:http";
import { networkInterfaces } from "node:os";
import { extname, join, normalize } from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { randomBytes } from "node:crypto";
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import { wasiImports } from "../../../web/kernel/kernel.js";
import { createIdentity, createRealm, fingerprint } from "../../../web/kernel/realm.js";
import { instantiate as instantiateLead } from "../gen/lead/lead.js";
import { instantiate as instantiateWorker } from "../gen/worker/worker.js";
import { startTabRealm } from "../web/src/tab-realm.js";
import { createRelay } from "./relay.mjs";

const here = fileURLToPath(new URL(".", import.meta.url));
const shim = { cli, clocks, io, random };
const core = (name) => async (path) => WebAssembly.compile(await readFile(join(here, "../gen", name, path)));

/** What the local realm lets out and in. It asks workers to count; it serves nobody. */
const LEAD_POLICY = {
  egress: [{ to: { labels: { role: "worker" } }, verbs: ["primes.count"] }],
  ingress: [],
};

function option(name, fallback) {
  const i = process.argv.indexOf("--" + name);
  if (i < 0) return fallback;
  const value = process.argv[i + 1];
  return value === undefined || value.startsWith("--") ? true : value;
}

const port = Number(option("port", 8790));
const token = String(option("token", randomBytes(4).toString("hex")));
const allowUnsigned = Boolean(option("allow-unsigned", false));
const message = option("message", null);
const waitFor = Number(option("wait", 0));
const localWorkers = Number(option("local-workers", 0));
const faultyWorkers = Number(option("faulty", 0));
let dispatch = String(option("mode", "all"));

// ---- the lead agent: a WASM component with no way out except its replies ----
const unused = (what) => () => { throw { tag: "not-found", val: `the lead agent has no ${what}` }; };
const lead = (await instantiateLead(core("lead"), wasiImports(shim, {
  "agentwos:core/llm": { complete: unused("model") },
  "agentwos:core/tool": { describe: () => "{}", invoke: unused("tool") },
}))).agent;

// ---- the realm and its relay ----
const identity = await createIdentity("lead", { role: "lead" });
let relay;
const realm = createRealm({
  identity,
  policy: { ...LEAD_POLICY, allowUnsigned },
  send: (to, msg) => relay.send(to, msg),
});

const TYPES = { ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm" };
const dist = join(here, "../web/dist");
const server = createServer(async (request, response) => {
  const path = normalize(decodeURIComponent(new URL(request.url, "http://x").pathname));
  const file = join(dist, path === "/" ? "index.html" : path);
  if (!file.startsWith(dist)) return response.writeHead(403).end();
  try {
    const body = await readFile(file);
    response.writeHead(200, { "content-type": TYPES[extname(file)] ?? "application/octet-stream", "cache-control": "no-store" }).end(body);
  } catch {
    response.writeHead(404).end("not found");
  }
});

const workers = () => [...realm.peers.values()].filter((p) => p.labels?.role === "worker");
let rosterChanged = () => {};
relay = createRelay({
  server,
  token,
  allowUnsigned,
  local: { advert: realm.advert(), receive: (msg) => realm.receive(msg) },
  onRoster: (peers) => { realm.setPeers(peers); rosterChanged(); },
});
await new Promise((resolve) => server.listen(port, resolve));

// ---- one request: the lead plans, the kernel carries the pieces, the lead answers ----
async function run(text) {
  const system = { role: "system", content: JSON.stringify({ members: workers().length }) };
  const first = JSON.parse(lead.run([system, { role: "user", content: text }]));
  if (first.done) return { text: first.done };

  const { pool, verb, bodies } = first.ask;
  const started = Date.now();
  const { values, members, sent, dissent } = await realm.askPool(pool, verb, bodies, { dispatch, parent: `request-${started}` });
  const final = JSON.parse(lead.run([system, { role: "user", content: text }, { role: "tool", content: JSON.stringify(values) }]));

  const intents = Object.values(sent).reduce((a, b) => a + b, 0);
  return { text: final.done, total: final.total, ms: Date.now() - started, pieces: bodies.length, intents, members, perMember: Object.entries(sent), dissent };
}

function report(r) {
  console.log(r.text);
  if (!r.ms) return;
  console.log(`  ${r.pieces} pieces, ${r.intents} intents to ${r.members.length} realms, ${r.ms} ms, dispatch ${dispatch}`);
  console.log("  " + r.perMember.map(([name, n]) => `${name}: ${n}`).join("  "));
  for (const [name, n] of Object.entries(r.dissent)) console.log(`  ${name} disagreed with the majority ${n} time(s)`);
}

function who() {
  if (!workers().length) return console.log("  no workers yet");
  for (const p of workers()) console.log(`  ${p.name.padEnd(14)} key ${fingerprint(p.key)}  serves ${p.serves.join(", ")}`);
}

// ---- optional in-process workers, for trying it without a browser ----
for (let i = 0; i < localWorkers; i++) {
  const worker = (await instantiateWorker(core("worker"), wasiImports(shim))).tool;
  await startTabRealm({
    url: `ws://localhost:${port}/relay`,
    token,
    name: `node-${i + 1}`,
    worker,
    faulty: () => i < faultyWorkers,
  });
}

if (waitFor) {
  await new Promise((resolve) => {
    rosterChanged = () => workers().length >= waitFor && resolve();
    rosterChanged();
  });
}

if (message) {
  try {
    const r = await run(String(message));
    report(r);
    if (option("json", false)) console.log(JSON.stringify({ total: r.total, dissent: r.dissent, intents: r.intents }));
  } catch (e) {
    console.log("error: " + e.message);
    process.exitCode = 1;
  }
  relay.close();
  server.close();
  process.exit();
}

console.log(`Tab Swarm: local realm "lead", key ${fingerprint(identity.key)}`);
console.log(`\nOpen this in one or more browsers, then come back here:\n  http://localhost:${port}/?token=${token}`);
const lan = Object.values(networkInterfaces()).flat().filter((n) => n && n.family === "IPv4" && !n.internal).map((n) => n.address);
if (lan.length) {
  console.log(`Other devices on your network${allowUnsigned ? "" : " (start with --allow-unsigned; a page off localhost cannot sign)"}:`);
  for (const address of lan) console.log(`  http://${address}:${port}/?token=${token}`);
}
console.log(`\nType: primes 5000000    Commands: /who  /mode all|first|quorum  /probe  /ledger  /exit\n`);

rosterChanged = () => console.log(`\n  [${workers().length} worker realm(s): ${workers().map((p) => p.name).join(", ") || "none"}]`);
const lines = createInterface({ input: process.stdin, output: process.stdout, prompt: "> " });
lines.prompt();
for await (const raw of lines) {
  const line = raw.trim();
  try {
    if (line === "/exit" || line === "/quit") break;
    else if (line === "/who") who();
    else if (line.startsWith("/mode")) {
      const mode = line.split(/\s+/)[1];
      if (["all", "any", "first", "quorum"].includes(mode)) dispatch = mode;
      console.log(`  dispatch: ${dispatch}`);
    } else if (line === "/probe") {
      // The lead asks a worker for something its own policy does not allow.
      const target = workers()[0]?.name;
      if (!target) console.log("  no workers yet");
      else await realm.ask(target, "files.read", { path: "/etc/passwd" }).catch((e) => console.log("  " + e.message));
    } else if (line.startsWith("/ledger")) {
      const n = Number(line.split(/\s+/)[1]) || 10;
      for (const e of realm.ledger.slice(-n)) {
        console.log(`  ${e.at.slice(11, 23)} ${e.decision.padEnd(5)} ${e.from} -> ${e.to} ${e.verb} (${e.by})${e.ms !== undefined ? " " + e.ms + " ms" : ""}${e.error ? " error: " + e.error : ""}`);
      }
    } else if (line) report(await run(line));
  } catch (e) {
    console.log("  error: " + e.message);
  }
  lines.prompt();
}
relay.close();
server.close();
process.exit();
