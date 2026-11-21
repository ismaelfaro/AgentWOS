// End-to-end check of duck soccer in Node with the real Needle model:
// the soccer stage (bodies and footwork) + one actor per duck + the referee.
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
import { instantiate as instantiateStage } from "../gen/soccer-stage/soccer-stage.js";
import { instantiate as instantiateControl } from "../gen/referee/referee.js";

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
const stage = (await instantiateStage(load("soccer-stage"), wasiImports(shim))).stage;
const control = (await instantiateControl(load("referee"), wasiImports(shim))).control;
const actor = (await instantiateActor(load("actor"), wasiImports(shim, { "agentwos:core/tool-llm@0.1.0": needle }))).actor;
const kernel = createKernel({ stage, actors: [actor, actor, actor, actor], controls: [{ name: "referee", control }] });

let failed = 0;
function check(label, ok, detail = "") {
  if (!ok) failed++;
  console.log(`${ok ? "PASS" : "FAIL"} ${label}${detail ? "  " + detail : ""}`);
}
const calls = (r) => r.turns.flatMap((t) => t.attempts.flatMap((a) => a.calls.map((c) => ({ agent: t.agent, ...c }))));
function run(text) {
  const r = kernel.cue(text);
  const lines = r.turns.flatMap((t) => t.attempts.flatMap((a) => a.calls.length
    ? a.calls.map((c) => `${t.agent}: ${c.call.name} -> ${c.delivered ? c.delivered.name : "blocked"} conf=${a.confidence.toFixed(2)} ${c.verdicts.map((v) => v.verdict).join(",")}`)
    : [`${t.agent}: ignored "${a.clause}" (${a.ignored})`]));
  console.log(`\n> ${text}  [${r.turns.map((t) => t.latency_ms + "ms").join(" ")}]\n  ${lines.join("\n  ") || "(no duck addressed)"}`);
  return r;
}
const asked = (text, agent) => calls(run(text)).filter((c) => c.agent === agent).map((c) => c.call.name);
const tick = (n) => { let f; for (let i = 0; i < n; i++) f = kernel.render(i); return f; };
const duck = (name) => kernel.state().ducks.find((d) => d.agent === name);

check("four ducks in two teams", kernel.agents.length === 4 && kernel.state().ducks.filter((d) => d.team === "Sky").length === 2);
check("the referee describes itself", kernel.controls[0].title === "Referee" && kernel.controls[0].rules.length === 3);

// Before the whistle nobody may attack.
const early = calls(run("Pip attack"));
check("no attack before the whistle", early.length === 1 && early[0].delivered === null && early[0].verdicts[0].verdict === "block");
tick(15);

// What the brain makes of each order.
for (const [text, agent, tool] of [
  ["Pip attack", "Pip", "attack"],
  ["Lila defend the goal", "Lila", "defend"],
  ["Momo pass the ball", "Momo", "pass_ball"],
  ["Nilo stop", "Nilo", "stop"],
  ["Momo attack", "Momo", "attack"],
  ["Nilo defend the goal", "Nilo", "defend"],
  ["Pip pass the ball", "Pip", "pass_ball"],
  ["the purple duck defend", "Lila", "defend"],
]) {
  const got = asked(text, agent);
  check(`"${text}" -> ${agent}.${tool}`, got.length === 1 && got[0] === tool, got.join(","));
}
for (const text of kernel.setup.suggestions) check(`suggestion works: ${text}`, calls(run(text)).length >= 1);

// One attacker per team: the second call becomes defend.
run("Everyone stop");
run("Pip attack");
const second = calls(run("Lila attack"))[0];
check("a second attacker is turned into a defender", second.delivered?.name === "defend" && second.verdicts[0].verdict === "modify");
check("a team name reaches both its ducks", calls(run("Sun defend the goal")).filter((c) => ["Momo", "Nilo"].includes(c.agent)).length === 2);
check("a cue that names nobody reaches none", calls(run("attack")).length === 0);

// A coached duck keeps its order: its own urges wait.
run("Everyone stop");
run("Pip defend the goal");
let pipUrges = 0;
for (let i = 0; i < 60; i++) pipUrges += kernel.render(i).urges.filter((u) => kernel.agents[u.agent].agent === "Pip").length;
check("a coached duck holds its order", pipUrges === 0 && duck("Pip").action === "defend");

// A match played by the ducks' own urges: every urge must be understood, and goals must come.
let urges = 0, understood = 0, goals = 0, kicks = 0, falls = 0, helped = 0;
const missed = new Set();
for (let i = 0; i < 12000; i++) {
  const f = kernel.render(i);
  for (const e of f.events) {
    if (e.goal) goals++;
    if (e.text === "kicked the ball") kicks++;
    if (e.text.startsWith("fell")) falls++;
    if (e.text.startsWith("the referee helps")) helped++;
  }
  for (const u of f.urges) {
    urges++;
    const name = kernel.agents[u.agent].agent;
    const got = calls(kernel.cue(u.text)).filter((c) => c.agent === name);
    const want = { attack: "attack", defend: "defend", pass: "pass_ball", stop: "stop" }[u.text.split(" ")[1]];
    if (got.length === 1 && got[0].call.name === want) understood++; else missed.add(`${u.text} -> ${got.map((c) => c.call.name).join(",") || "nothing"}`);
  }
}
const end = kernel.state();
console.log(`\n20 minutes: ${urges} urges, ${kicks} kicks, ${goals} goals, ${falls} falls, ${helped} helped up; score Sky ${end.score.Sky} - Sun ${end.score.Sun}`);
check("ducks voice urges", urges >= 60, `${urges}`);
check("the brain understands every urge", understood === urges, `${understood}/${urges}${missed.size ? " missed: " + [...missed].slice(0, 6).join(" | ") : ""}`);
check("ducks kick the ball", kicks >= 10, `${kicks} kicks`);
check("goals are scored", goals >= 1 && goals === end.score.Sky + end.score.Sun, `${goals} goals`);
check("ducks stay on the pitch", end.ducks.every((d) => d.x > 0 && d.x < 4 && d.y > 0 && d.y < 2.6));

console.log(`\n${failed ? failed + " FAILED" : "all checks passed"}`);
process.exit(failed ? 1 : 0);
