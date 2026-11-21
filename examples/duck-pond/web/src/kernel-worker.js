// What both pages' kernel workers share: Needle, the WASI shim and the message
// loop. A page's worker passes its own stage and control.
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import createCactus from "../vendor/cactus.js";
import { coreModuleLoader, createKernel, wasiImports } from "../../../../web/kernel/kernel.js";
import { createNeedle, fetchCached } from "../../../../web/kernel/needle.js";
import { instantiate as instantiateActor } from "../gen/actor/actor.js";

// The page's own copy is tried first; Hugging Face is the fallback.
const MODEL_URLS = ["../model/needle3.cact", "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact"];
const CACHE = "duck-pond-v1";
const base = self.location.href;

async function fetchFirst(urls, cache, onProgress) {
  let lastError = null;
  for (const url of urls) {
    try {
      return await fetchCached(new URL(url, base).href, cache, onProgress);
    } catch (e) {
      lastError = e;
    }
  }
  throw lastError;
}

/**
 * Starts the kernel worker for one page.
 * @param instantiateStage    jco `instantiate` of the page's stage component
 * @param instantiateControl  jco `instantiate` of its control component
 * @param stageDir, controlDir, controlName  where their core wasm files live and the control's name
 */
export function runKernelWorker({ instantiateStage, instantiateControl, stageDir, controlDir, controlName }) {
  let kernel = null;

  async function init() {
    const progress = (label) => (loaded, total) => postMessage({ type: "progress", label, loaded, total });
    const wasmBinary = await fetchCached(new URL("../cactus.wasm", base).href, CACHE, progress("engine"));
    const modelBytes = await fetchFirst(MODEL_URLS, CACHE, progress("model"));
    postMessage({ type: "progress", label: "starting", loaded: 1, total: 1 });

    const needle = await createNeedle({ createCactus, wasmBinary, modelBytes });
    const shim = { cli, clocks, io, random };
    const wasm = (name) => coreModuleLoader(new URL(`wasm/${name}/`, base));

    const stage = (await instantiateStage(wasm(stageDir), wasiImports(shim))).stage;
    const control = (await instantiateControl(wasm(controlDir), wasiImports(shim))).control;
    const agentCount = JSON.parse(stage.setup("{}")).agents.length;
    // The brains share one actor instance: an actor keeps no state between calls.
    const shared = (await instantiateActor(wasm("actor"), wasiImports(shim, { "agentwos:core/tool-llm": needle }))).actor;
    const actors = Array.from({ length: agentCount }, () => shared);

    kernel = createKernel({ stage, actors, controls: [{ name: controlName, control }] });
    postMessage({ type: "ready", setup: kernel.setup, controls: kernel.controls, state: kernel.state() });
  }

  function fail(e) {
    postMessage({ type: "error", message: String(e?.message ?? e?.payload ?? e) });
  }

  self.onmessage = ({ data }) => {
    if (data.type === "init") return init().catch(fail);
    if (!kernel) return fail(new Error("the kernel is still starting"));
    try {
      if (data.type === "cue") {
        const started = performance.now();
        const result = kernel.cue(data.text);
        postMessage({ type: "cue-result", id: data.id, result, ms: Math.round(performance.now() - started) });
      } else if (data.type === "render") {
        // `steps` body ticks in one message: how the page runs the world faster than real time.
        const steps = Math.max(1, Math.min(data.steps || 1, 8));
        let frame = null;
        const events = [], urges = [];
        for (let k = 0; k < steps; k++) {
          frame = kernel.render(data.tick + k);
          events.push(...(frame.events || []));
          urges.push(...(frame.urges || []));
        }
        postMessage({ type: "frame", frame: { ...frame, events, urges } });
      }
    } catch (e) {
      fail(e);
    }
  };
}
