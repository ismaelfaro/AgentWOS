// Body worker: the stage (the ducks' bodies) and the duck safety control. It never
// waits for a model: the brains think in brain.js, and the page passes their
// decisions here (`deliver`). So frames keep coming while a duck is thinking.
import * as cli from "@bytecodealliance/preview2-shim/cli";
import * as clocks from "@bytecodealliance/preview2-shim/clocks";
import * as io from "@bytecodealliance/preview2-shim/io";
import * as random from "@bytecodealliance/preview2-shim/random";
import { coreModuleLoader, createKernel, wasiImports } from "../../../../web/kernel/kernel.js";
import { instantiate as instantiateStage } from "../gen/stage/stage.js";
import { instantiate as instantiateControl } from "../gen/duck-safety/duck-safety.js";
import { deliver } from "./split.js";

const base = self.location.href;
let kernel = null;

async function init() {
  const shim = { cli, clocks, io, random };
  const wasm = (name) => coreModuleLoader(new URL(`wasm/${name}/`, base));
  const stage = (await instantiateStage(wasm("stage"), wasiImports(shim))).stage;
  const control = (await instantiateControl(wasm("duck-safety"), wasiImports(shim))).control;
  // No actors here: a cue reaches the bodies through `deliver`, after a brain has thought.
  kernel = createKernel({ stage, actors: [], controls: [{ name: "duck-safety", control }] });
  // Each brief as the stage wrote it: Needle is sensitive to the key order of the tool list.
  const briefs = kernel.agents.map((_, i) => stage.brief(i));
  postMessage({ type: "ready", setup: kernel.setup, controls: kernel.controls, state: kernel.state(), briefs });
}

function fail(e) {
  postMessage({ type: "error", message: String(e?.message ?? e?.payload ?? e) });
}

self.onmessage = ({ data }) => {
  if (data.type === "init") return init().catch(fail);
  if (!kernel) return fail(new Error("the bodies are still starting"));
  try {
    if (data.type === "render") {
      postMessage({ type: "frame", frame: kernel.render(data.tick) });
    } else if (data.type === "deliver") {
      postMessage({ type: "delivered", id: data.id, turns: deliver(kernel, data.turns) });
    }
  } catch (e) {
    fail(e);
  }
};
