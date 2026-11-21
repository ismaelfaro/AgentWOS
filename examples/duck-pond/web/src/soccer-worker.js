// The match's kernel worker: the soccer stage and the referee control.
import { runKernelWorker } from "./kernel-worker.js";
import { instantiate as instantiateStage } from "../gen/soccer-stage/soccer-stage.js";
import { instantiate as instantiateControl } from "../gen/referee/referee.js";

runKernelWorker({ instantiateStage, instantiateControl, stageDir: "soccer-stage", controlDir: "referee", controlName: "referee" });
