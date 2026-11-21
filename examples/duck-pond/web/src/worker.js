// The pond's kernel worker: the pond stage and the duck safety control.
import { runKernelWorker } from "./kernel-worker.js";
import { instantiate as instantiateStage } from "../gen/stage/stage.js";
import { instantiate as instantiateControl } from "../gen/duck-safety/duck-safety.js";

runKernelWorker({ instantiateStage, instantiateControl, stageDir: "stage", controlDir: "duck-safety", controlName: "duck-safety" });
