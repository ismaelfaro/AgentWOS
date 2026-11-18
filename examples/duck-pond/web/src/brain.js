// Brain worker: Needle and the actor. Thinking takes about 0.8 s per duck addressed
// and blocks this thread, which is fine: nothing else runs here. The bodies are in
// worker.js, so the animation does not wait.
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import createCactus from "../vendor/cactus.js";
import { coreModuleLoader, wasiImports } from "../../../../web/kernel/kernel.js";
import { createNeedle, fetchCached } from "../../../../web/kernel/needle.js";
import { instantiate as instantiateActor } from "../gen/actor/actor.js";
import { think } from "./split.js";

// The page's own copy is tried first; Hugging Face is the fallback.
const MODEL_URLS = ["../model/needle3.cact", "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact"];
const CACHE = "duck-pond-v1";
const base = self.location.href;
let actor = null;

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

async function init() {
  const progress = (label) => (loaded, total) => postMessage({ type: "progress", label, loaded, total });
  const wasmBinary = await fetchCached(new URL("../cactus.wasm", base).href, CACHE, progress("engine"));
  const modelBytes = await fetchFirst(MODEL_URLS, CACHE, progress("model"));
  postMessage({ type: "progress", label: "starting", loaded: 1, total: 1 });

  const needle = await createNeedle({ createCactus, wasmBinary, modelBytes });
  const shim = { cli, clocks, io, random };
  const wasm = (name) => coreModuleLoader(new URL(`wasm/${name}/`, base));
  // The four brains share one actor instance: an actor keeps no state between calls.
  actor = (await instantiateActor(wasm("actor"), wasiImports(shim, { "agentwos:core/tool-llm": needle }))).actor;
  postMessage({ type: "ready" });
}

function fail(e) {
  postMessage({ type: "error", message: String(e?.message ?? e?.payload ?? e) });
}

self.onmessage = ({ data }) => {
  if (data.type === "init") return init().catch(fail);
  if (!actor) return fail(new Error("the brains are still loading"));
  try {
    if (data.type === "think") {
      postMessage({ type: "thought", id: data.id, turns: think(actor, data.briefs, data.agents, data.text) });
    }
  } catch (e) {
    fail(e);
  }
};
