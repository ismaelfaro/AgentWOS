// End-to-end check of the kernel in Node with the real Needle model:
// stage + actors + sound engineer, driven by conductor cues.
// Run after ../build.sh. Needs vendor/needle3.cact (downloaded on first run).
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
import { instantiate as instantiateControl } from "../gen/sound-engineer/sound-engineer.js";

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
const control = (await instantiateControl(load("sound-engineer"), wasiImports(shim))).control;
const actors = [];
for (let i = 0; i < 3; i++) {
  const root = await instantiateActor(load("actor"), wasiImports(shim, { "agentwos:core/tool-llm@0.1.0": needle }));
  actors.push(root.actor);
}
const kernel = createKernel({ stage, actors, controls: [{ name: "sound-engineer", control }] });

let failed = 0;
function check(label, ok, detail = "") {
  if (!ok) failed++;
  console.log(`${ok ? "PASS" : "FAIL"} ${label}${detail ? "  " + detail : ""}`);
}
function run(text) {
  const r = kernel.cue(text);
  const summary = r.turns.flatMap((t) => t.attempts.flatMap((a) => a.calls.length
    ? a.calls.map((c) => `${t.agent}: ${c.call.name}(${Object.values(c.call.arguments).join(",")}) conf=${a.confidence.toFixed(2)} ${c.verdicts.map((v) => v.verdict).join(",")}${c.error ? " ERR " + c.error : ""}`)
    : [`${t.agent}: ignored "${a.clause}" (${a.ignored}, conf=${a.confidence.toFixed(2)})`]));
  console.log(`\n> ${text}  [${r.turns.map((t) => t.latency_ms + "ms").join(" ")}]\n  ${summary.join("\n  ") || "(no agent addressed)"}`);
  return r;
}

const applied = (r) => r.turns.flatMap((t) => t.attempts.flatMap((a) => a.calls)).filter((c) => c.delivered && !c.error);

check("three agents", kernel.agents.length === 3);
check("render has notes", kernel.render(0).notes.length > 0);

// Every suggestion the UI offers must produce at least one applied call.
for (const cue of kernel.setup.suggestions) {
  check(`suggestion works: ${cue}`, applied(run(cue)).length > 0);
}

// Phrasings that real users tried.
let r = run("drums double time");
check("drums double_time", r.state.drums === "double_time");
r = run("make drum jazz");
check("drums jazz", r.state.drums === "jazz");
r = run("make drum random");
check("drums random", r.state.drums === "random");
r = run("speed to 120");
check("speed to 120", r.state.tempo === 120);
r = run("bass octaves G");
check("bass octaves on G", r.state.bass_root === "G" && r.state.bass_movement === "octaves");
r = run("lead goes jazz");
check("lead jazzy", r.state.lead === "jazzy");
r = run('drums double time"');
check("stray quote is ignored", r.state.drums === "double_time");

// Stage and actor behaviour.
r = run("Bass walking on F");
r = run("bass A");
check("root-only cue keeps the movement", r.state.bass_movement === "walking", `movement=${r.state.bass_movement}`);
check("engineer moves A into the key (Ab)", r.state.bass_root === "Ab", `root=${r.state.bass_root}`);
r = run("Bass walking on F");
r = run("Bass walking on F");
check("repeat reports no change", applied(r)[0]?.result.startsWith("no change"), applied(r)[0]?.result);
r = run("Drums play a breakbeat and make the melody bright");
check("compound cue splits across agents", r.state.drums === "breakbeat" && r.state.lead === "bright");
r = run("Silence the drums");
check("only drums silenced", r.state.drums === "silent" && r.state.lead === "bright");

// Sound engineer.
r = run("Set the tempo to 300");
check("engineer clamps tempo to 160", r.state.tempo === 160, `tempo=${r.state.tempo}`);
r = run("Lead goes frantic");
check("engineer blocks frantic at 160 bpm", r.state.lead === "bright");
r = run("Set the tempo to 120");
r = run("Lead goes frantic");
check("frantic allowed at 120 bpm", r.state.lead === "frantic");
r = run("Tell me a joke");
check("unrelated cue changes nothing", r.turns.every((t) => t.attempts.length === 0));

console.log(`\n${failed ? failed + " FAILED" : "all checks passed"}`);
process.exit(failed ? 1 : 0);
