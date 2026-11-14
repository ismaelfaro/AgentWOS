// Kernel worker: Needle and every WASM component run here, off the UI thread,
// so the audio scheduler on the page never stalls while an agent is thinking.
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import createCactus from "../vendor/cactus.js";
import { coreModuleLoader, createKernel, wasiImports } from "../../../../web/kernel/kernel.js";
import { createNeedle, fetchCached } from "../../../../web/kernel/needle.js";
import { instantiate as instantiateActor } from "../gen/actor/actor.js";
import { instantiate as instantiateStage } from "../gen/stage/stage.js";
import { instantiate as instantiateControl } from "../gen/sound-engineer/sound-engineer.js";

const MODEL_URL = "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact";
const CACHE = "tab-jam-v1";
const base = self.location.href;
let kernel = null;

async function init() {
  const progress = (label) => (loaded, total) => postMessage({ type: "progress", label, loaded, total });
  const wasmBinary = await fetchCached(new URL("../cactus.wasm", base).href, CACHE, progress("engine"));
  const modelBytes = await fetchCached(MODEL_URL, CACHE, progress("model"));
  postMessage({ type: "progress", label: "starting", loaded: 1, total: 1 });

  const needle = await createNeedle({ createCactus, wasmBinary, modelBytes });
  const shim = { cli, clocks, io, random };
  const wasm = (name) => coreModuleLoader(new URL(`wasm/${name}/`, base));

  const stage = (await instantiateStage(wasm("stage"), wasiImports(shim))).stage;
  const control = (await instantiateControl(wasm("sound-engineer"), wasiImports(shim))).control;
  const agentCount = JSON.parse(stage.setup("{}")).agents.length;
  const actors = [];
  for (let i = 0; i < agentCount; i++) {
    // One actor instance per agent: separate memory, same tool-llm service.
    const root = await instantiateActor(wasm("actor"), wasiImports(shim, { "agentwos:core/tool-llm": needle }));
    actors.push(root.actor);
  }

  kernel = createKernel({ stage, actors, controls: [{ name: "sound-engineer", control }] });
  postMessage({
    type: "ready",
    setup: kernel.setup,
    state: kernel.state(),
    briefs: kernel.briefs(),
    modelBytes: modelBytes.length,
  });
}

function fail(e) {
  postMessage({ type: "error", message: String(e?.message ?? e), stack: String(e?.stack ?? "") });
}

self.onmessage = ({ data }) => {
  if (data.type !== "init" && !kernel) return fail(new Error("the kernel is still starting"));
  try {
    if (data.type === "init") {
      init().catch(fail);
    } else if (data.type === "cue") {
      postMessage({ type: "cue-result", id: data.id, result: kernel.cue(data.text) });
    } else if (data.type === "call") {
      postMessage({ type: "cue-result", id: data.id, result: kernel.call(data.agent, data.call, "pad") });
    } else if (data.type === "render") {
      postMessage({ type: "bar", bar: data.bar, data: kernel.render(data.bar) });
    }
  } catch (e) {
    fail(e);
  }
};
