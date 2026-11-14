// End-to-end check of the kernel in Node with the real Needle model:
// stage + actors + sound engineer, driven by conductor cues.
// Run after ../build.sh. Needs vendor/needle3.cact (downloaded on first run).
import { readFile, writeFile, access } from "node:fs/promises";
import { createRequire } from "node:module";
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
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

const shim = { cli, clocks, io };
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

check("three agents", kernel.agents.length === 3);
check("render has notes", kernel.render(0).notes.length > 0);

let r = run("Make the drums half time");
check("drums half_time", r.state.drums === "half_time");
r = run("Bass walking on F");
check("bass walking on F", r.state.bass_root === "F" && r.state.bass_movement === "walking");
r = run("Make the melody dark");
check("lead dark", r.state.lead === "dark");
r = run("Drums play a breakbeat and make the melody bright");
check("compound cue splits across agents", r.state.drums === "breakbeat" && r.state.lead === "bright");
r = run("Silence the drums");
check("only drums silenced", r.state.drums === "silent" && r.state.lead === "bright");
r = run("Set the tempo to 300");
check("engineer clamps tempo to 160", r.state.tempo === 160, `tempo=${r.state.tempo}`);
r = run("Lead goes frantic");
check("engineer blocks frantic at 160 bpm", r.state.lead === "bright");
r = run("Set the tempo to 120");
check("tempo 120", r.state.tempo === 120);
r = run("Lead goes frantic");
check("frantic allowed at 120 bpm", r.state.lead === "frantic");
r = run("Tell me a joke");
check("unrelated cue changes nothing", r.turns.every((t) => t.attempts.length === 0));

console.log(`\n${failed ? failed + " FAILED" : "all checks passed"}`);
process.exit(failed ? 1 : 0);
