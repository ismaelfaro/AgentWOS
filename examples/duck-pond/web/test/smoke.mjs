// End-to-end check of the browser kernel in Node with the real Needle model:
// stage (the bodies) + one actor per duck (the brains) + duck safety.
// Run after ../build-web.sh. Needs vendor/needle3.cact (downloaded on first run).
import { readFile, writeFile, access } from "node:fs/promises";
import { createRequire } from "node:module";
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import { createKernel, wasiImports } from "../../../../web/kernel/kernel.js";
import { createNeedle } from "../../../../web/kernel/needle.js";
import { instantiate as instantiateActor } from "../gen/actor/actor.js";
import { instantiate as instantiateStage } from "../gen/stage/stage.js";
import { instantiate as instantiateControl } from "../gen/duck-safety/duck-safety.js";

const here = new URL(".", import.meta.url);
const MODEL_URL = "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact";
const modelPath = new URL("../vendor/needle3.cact", here);
try { await access(modelPath); } catch {
  console.log("downloading needle3.cact (35 MB)...");
  await writeFile(modelPath, Buffer.from(await (await fetch(MODEL_URL)).arrayBuffer()));
}

const createCactus = createRequire(import.meta.url)("../vendor/cactus.js");
const needle = await createNeedle({
  createCactus,
  wasmBinary: await readFile(new URL("../vendor/cactus.wasm", here)),
  modelBytes: await readFile(modelPath),
});

const shim = { cli, clocks, io, random };
const load = (dir) => async (path) => WebAssembly.compile(await readFile(new URL(`../gen/${dir}/${path}`, here)));
const stage = (await instantiateStage(load("stage"), wasiImports(shim))).stage;
const control = (await instantiateControl(load("duck-safety"), wasiImports(shim))).control;
// An actor keeps no state between calls, so the four ducks share one instance.
const actor = (await instantiateActor(load("actor"), wasiImports(shim, { "agentwos:core/tool-llm@0.1.0": needle }))).actor;
const kernel = createKernel({ stage, actors: [actor, actor, actor, actor], controls: [{ name: "duck-safety", control }] });

let failed = 0;
function check(label, ok, detail = "") {
  if (!ok) failed++;
  console.log(`${ok ? "PASS" : "FAIL"} ${label}${detail ? "  " + detail : ""}`);
}
const calls = (r) => r.turns.flatMap((t) => t.attempts.flatMap((a) => a.calls.map((c) => ({ agent: t.agent, ...c }))));
function run(text) {
  const r = kernel.cue(text);
  const lines = r.turns.flatMap((t) => t.attempts.flatMap((a) => a.calls.length
    ? a.calls.map((c) => `${t.agent}: ${c.call.name}(${Object.values(c.call.arguments ?? {}).join(",")}) conf=${a.confidence.toFixed(2)} ${c.verdicts.map((v) => v.verdict).join(",")}${c.error ? " ERR " + c.error : ""}`)
    : [`${t.agent}: ignored "${a.clause}" (${a.ignored})`]));
  console.log(`\n> ${text}  [${r.turns.map((t) => t.latency_ms + "ms").join(" ")}]\n  ${lines.join("\n  ") || "(no duck addressed)"}`);
  return r;
}
/** The one delivered call a cue should produce for one duck. */
function expectCall(text, agent, name, args = {}) {
  const got = calls(run(text)).filter((c) => c.agent === agent && c.delivered && !c.error);
  const ok = got.length === 1 && got[0].delivered.name === name
    && Object.entries(args).every(([k, v]) => got[0].delivered.arguments?.[k] === v);
  check(`"${text}" -> ${agent}.${name}(${Object.values(args).join(",")})`, ok);
}
const tick = (n) => { let f; for (let i = 0; i < n; i++) f = kernel.render(i); return f; };
const duck = (name) => kernel.state().ducks.find((d) => d.agent === name);

check("four ducks", kernel.agents.length === 4, kernel.agents.map((a) => a.agent).join(" "));
check("the control describes itself", kernel.controls[0].title === "Duck safety" && kernel.controls[0].rules.length === 4);

// Every suggestion the page offers must work.
for (const text of kernel.setup.suggestions) {
  const got = calls(run(text)).filter((c) => c.delivered && !c.error);
  check(`suggestion works: ${text}`, got.length >= 1);
}

expectCall("Pip walk to the cushion", "Pip", "walk_to", { place: "cushion" });
expectCall("Momo kick the ball", "Momo", "kick_ball");
expectCall("Nilo take a nap", "Nilo", "nap");
expectCall("Momo do the zoomies", "Momo", "zoomies");
expectCall("Pip look around", "Pip", "look_around");
expectCall("Lila walk to the plant", "Lila", "walk_to", { place: "plant" });
expectCall("the blue duck walk to the sofa", "Pip", "walk_to", { place: "sofa" });

const both = calls(run("Pip walk to the shelf and Nilo stop")).filter((c) => c.delivered);
check("two ducks in one cue", both.length === 2 && both[0].agent === "Pip" && both[1].agent === "Nilo");
check("quack is not offered as a tool", !JSON.stringify(kernel.setup.suggestions).includes("quack"));
check("everyone answers 'Everyone stop'", calls(run("Everyone stop")).filter((c) => c.delivered?.name === "stop").length === 4);
check("a cue that names no duck reaches none", calls(run("walk to the sofa")).length === 0);

// The body runs what the brain asked for.
run("Everyone stop");
run("Pip walk to the plant");
const before = duck("Pip");
tick(60);
const after = duck("Pip");
check("Pip walked", Math.hypot(after.x - before.x, after.y - before.y) > 0.2, `${Math.hypot(after.x - before.x, after.y - before.y).toFixed(2)} m`);

// Every urge a duck's body can voice must be something its brain understands.
let urges = 0, understood = 0, quacks = 0;
const missed = new Set();
for (let i = 0; i < 1500; i++) {
  const frame = kernel.render(i);
  quacks += frame.events.filter((e) => e.text.startsWith("quacks:")).length;
  for (const u of frame.urges) {
    urges++;
    const got = calls(kernel.cue(u.text)).filter((c) => c.agent === kernel.agents[u.agent].agent);
    if (got.length === 1) understood++; else missed.add(u.text);
  }
}
check("idle ducks voice urges", urges >= 20, `${urges} urges in 150 s`);
check("the brain understands every urge", understood === urges, `${understood}/${urges}${missed.size ? " missed: " + [...missed].join(" | ") : ""}`);
const end = kernel.state().ducks;
const lang = kernel.render(2000).language;
const heardBy = lang.ducks.map((d) => d.means.filter(Boolean).length);
check("ducks learn words for places", heardBy.some((n) => n > 0), `words known per duck: ${heardBy.join(" ")}`);
check("ducks agree on some words", lang.agreement !== null, `agreement ${lang.agreement === null ? "n/a" : (lang.agreement * 100).toFixed(0) + "%"}`);
console.log("language: " + JSON.stringify(lang.ducks.map((d) => [d.agent, d.means])));
check("ducks quack on their own", quacks >= 3, `${quacks} quacks in 150 s`);
check("energy stays in range", end.every((d) => d.energy >= 0 && d.energy <= 1));
console.log("\n" + end.map((d) => `${d.agent}: ${d.action} ${d.policy} energy ${(d.energy * 100).toFixed(0)}%`).join("\n"));

// Safety: a tired duck is sent to nap, whatever its brain asks.
console.log(`\n${failed ? failed + " FAILED" : "all checks passed"}`);
process.exit(failed ? 1 : 0);
