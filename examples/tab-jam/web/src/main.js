// Tab Jam UI: sends cues to the kernel worker, plays what the stage renders
// with WebAudio, and shows what every agent and control did.

import { createPad } from "./pads.js";
import { createSignals } from "./signals.js";
import { createSynth } from "./synth.js";

const $ = (id) => document.getElementById(id);
const worker = new Worker(new URL("worker.js", import.meta.url), { type: "module" });

const COLORS = { Drummer: "--drums", Bassist: "--bass", Lead: "--lead" };
const ROWS = ["orchestra", "lead", "bass", "hat", "snare", "kick"];
const ROW_COLOR = { orchestra: "--orchestra", lead: "--lead", bass: "--bass", hat: "--drums", snare: "--drums", kick: "--drums" };
const ROW_OF = { drone: "orchestra" };

const MAX_LOG_ENTRIES = 40;
const callText = (c) => `${c.name}(${Object.entries(c.arguments).map(([k, v]) => `${k}: ${v}`).join(", ")})`;
const signals = createSignals($("band"));
let agents = [];
let briefs = [];
let pads = [];
let choices = [];
let padDescriptions = [];
let controls = [];
/** Verdict counts and the latest verdict per control, by control name. */
const tally = {};
const VERDICT_COLOR = { allow: "--ok", modify: "--warn", flag: "--warn", block: "--bad", error: "--bad" };
const VERDICT_WORD = { allow: "allowed", modify: "changed", flag: "flagged", block: "blocked", error: "failed" };
let state = null;
let cueId = 0;

function el(tag, attrs = {}, children = []) {
  const n = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (v === null || v === undefined || v === false) continue;
    if (k === "class") n.className = v;
    else if (k.startsWith("on")) n.addEventListener(k.slice(2), v);
    else n.setAttribute(k, v === true ? "" : v);
  }
  for (const c of [].concat(children)) if (c !== null && c !== undefined) n.append(c);
  return n;
}

const NEEDLE_URL = "https://cactuscompute.com/needle";

function showMeta() {
  if (!audio.metaBase) return;
  $("meta").replaceChildren(
    audio.metaBase + " - Thanks to ",
    el("a", { href: NEEDLE_URL, target: "_blank", rel: "noopener" }, audio.modelLabel),
  );
}

function setStatus(text, kind = "") {
  $("status").textContent = text;
  $("status").className = "pill " + kind;
}

// ---------- audio ----------
const audio = { ctx: null, synth: null, playing: false, nextBarTime: 0, bar: 0, bars: new Map(), last: null, now: null };

function initAudio() {
  audio.ctx = new AudioContext();
  audio.synth = createSynth(audio.ctx);
  audio.synth.setVolume(Number($("volume").value));
  loadLeadSample();
}

// A local recording can replace the synthesized lead. The samples folder is not
// part of the repository, so without it the synth voice plays.
async function loadLeadSample() {
  try {
    const meta = await (await fetch(new URL("samples/lead.json", import.meta.url))).json();
    const data = await (await fetch(new URL("samples/" + meta.file, import.meta.url))).arrayBuffer();
    audio.synth.setLeadSample(await audio.ctx.decodeAudioData(data), meta.midi);
    audio.leadSample = meta.file;
    audio.useSample = true;
    showMeta();
    renderChoices();
  } catch {
    audio.leadSample = null;
  }
}

function scheduleBar() {
  // If the worker is busy with a cue, repeat the previous bar instead of going silent.
  const data = audio.bars.get(audio.bar) ?? audio.last;
  if (!data) return;
  const step = 60 / data.tempo / 4;
  const start = audio.nextBarTime;
  audio.synth.setTempo(data.tempo);
  audio.synth.setLeadSound(audio.useSample ? "sample" : data.lead_sound ?? "saw");
  for (const n of data.notes) audio.synth.playNote(n, start + n.step * step, step);
  audio.now = { data, start, duration: step * 16, bar: audio.bar };
  // The bar clock: a faint spike from the drummer to everyone, on the downbeat.
  setTimeout(() => emit("tempo", 0.25), Math.max(0, (start - audio.ctx.currentTime) * 1000));
  audio.last = data;
  audio.bars.delete(audio.bar);
  audio.nextBarTime = start + step * 16;
  audio.bar += 1;
  worker.postMessage({ type: "render", bar: audio.bar });
}

setInterval(() => {
  if (audio.playing && audio.nextBarTime - audio.ctx.currentTime < 0.2) scheduleBar();
}, 40);

function startPlayback() {
  audio.playing = true;
  audio.nextBarTime = audio.ctx.currentTime + 0.15;
  audio.ctx.resume();
  $("toggle").textContent = "Pause";
}

function drawRoll() {
  requestAnimationFrame(drawRoll);
  const canvas = $("roll");
  if (canvas.offsetParent === null) return;
  const g = canvas.getContext("2d");
  const css = getComputedStyle(document.documentElement);
  const W = canvas.width, H = canvas.height, label = 64, rowH = H / ROWS.length, colW = (W - label) / 16;
  g.clearRect(0, 0, W, H);
  g.font = "13px system-ui";
  g.textBaseline = "middle";
  ROWS.forEach((row, r) => {
    g.fillStyle = css.getPropertyValue("--muted");
    g.fillText(row, 8, r * rowH + rowH / 2);
    for (let s = 0; s < 16; s++) {
      g.fillStyle = css.getPropertyValue(s % 4 === 0 ? "--line" : "--soft");
      g.fillRect(label + s * colW + 1, r * rowH + 3, colW - 2, rowH - 6);
    }
  });
  const now = audio.now;
  if (!now) return;
  for (const n of now.data.notes) {
    const row = ROW_OF[n.inst] ?? n.inst;
    const r = ROWS.indexOf(row);
    if (r < 0) continue;
    g.fillStyle = css.getPropertyValue(ROW_COLOR[row]);
    if (n.inst === "drone") {
      // One thin lane per section, drawn as its pulses across the bar.
      const lane = (rowH - 6) / n.count, y = r * rowH + 3 + (n.voice % n.count) * lane;
      g.globalAlpha = 0.8;
      for (let p = 0; p < n.len; p += n.pulse) g.fillRect(label + p * colW + 1, y, colW * n.pulse - 3, Math.max(1, lane - 1));
      continue;
    }
    g.globalAlpha = 0.45 + 0.55 * (n.vel / 127);
    g.fillRect(label + n.step * colW + 1, r * rowH + 3, colW * n.len - 2, rowH - 6);
  }
  g.globalAlpha = 1;
  const pos = (audio.ctx.currentTime - now.start) / now.duration;
  if (audio.playing && pos >= 0 && pos <= 1) {
    g.fillStyle = css.getPropertyValue("--accent");
    g.fillRect(label + pos * (W - label), 0, 2, H);
  }
  $("bar-label").textContent = `bar ${now.bar + 1} - ${now.data.tempo} bpm - C minor`;
}

// ---------- signals between agents ----------
const cardOf = (index) => (agents[index] ? $("agent-" + agents[index].agent) : null);

/** The resolved colour of a CSS variable, for SVG strokes and animations. */
const cssColor = (name) => getComputedStyle(document.documentElement).getPropertyValue(name).trim();
const colorOf = (index) => cssColor(COLORS[agents[index]?.agent] ?? "--orchestra");

/**
 * Sends a spike along every link that carries `signal`: from the agent that
 * owns the value to each agent that follows it. The links come from the stage.
 */
function emit(signal, strength) {
  for (const link of state?.links ?? []) {
    if (link.signal !== signal) continue;
    link.to.forEach((to, i) =>
      signals.spike(cardOf(link.from), cardOf(to), { color: colorOf(link.from), strength, delay: i * 30 }));
  }
}

/** Spikes for whatever changed between two states of the band. */
function emitChanges(before, after) {
  for (const link of after?.links ?? []) {
    if (before && before[link.state] !== after[link.state]) emit(link.signal, 1);
  }
}

/** A text cue travels from the cue box to every agent that took it. */
function emitCue(result) {
  if (result.source) return;
  result.turns.forEach((turn, i) => {
    if (!turn.attempts.length) return;
    signals.spike($("cue"), $("agent-" + turn.agent), { color: cssColor("--accent"), strength: 0.9, delay: i * 40 });
  });
}

/**
 * Every tool call goes to the controller that inspects it and comes back with
 * a verdict: the outbound spike has the agent's colour, the return one the
 * verdict's. A request to add an agent starts at the add tile.
 */
function emitVerdicts(result) {
  let n = 0;
  for (const turn of result.turns) {
    const index = agents.findIndex((a) => a.agent === turn.agent);
    const card = index >= 0 ? cardOf(index) : result.added ? $("agent-" + result.added.agent) : $("add-agent");
    const color = index >= 0 ? colorOf(index) : cssColor("--accent");
    for (const attempt of turn.attempts) {
      for (const call of attempt.calls) {
        for (const verdict of call.verdicts) {
          const controller = $("controller-" + verdict.module);
          const delay = 120 + n++ * 70;
          signals.spike(card, controller, { color, strength: 0.5, delay });
          signals.spike(controller, card, { color: cssColor(VERDICT_COLOR[verdict.verdict] ?? "--muted"), strength: 0.9, delay: delay + 380 });
        }
      }
    }
  }
}

// ---------- controller agents ----------
function recordVerdicts(result) {
  for (const turn of result.turns) {
    for (const attempt of turn.attempts) {
      for (const call of attempt.calls) {
        for (const verdict of call.verdicts) {
          const t = (tally[verdict.module] ??= { allow: 0, modify: 0, flag: 0, block: 0, error: 0, last: null });
          t[verdict.verdict] = (t[verdict.verdict] ?? 0) + 1;
          const what = verdict.verdict === "modify" && call.delivered ? `, now ${callText(call.delivered)}` : verdict.detail ? `: ${verdict.detail}` : "";
          t.last = { verdict: verdict.verdict, text: `${turn.agent} ${callText(call.call)} ${VERDICT_WORD[verdict.verdict] ?? verdict.verdict}${what}` };
        }
      }
    }
  }
}

function renderControllers() {
  for (const c of controls) {
    const t = tally[c.name];
    $("tally-" + c.name)?.replaceChildren(...["allow", "modify", "block", "flag"].map((v) =>
      el("span", { class: "tag " + (t?.[v] ? v : "") }, `${VERDICT_WORD[v]} ${t?.[v] ?? 0}`)));
    const last = $("verdict-" + c.name);
    if (last && t?.last) {
      last.textContent = t.last.text;
      last.className = "last verdict-" + t.last.verdict;
    }
  }
}

function controllerCards() {
  return controls.map((c) => el("div", { class: "agent controller", id: "controller-" + c.name, style: "--c:var(--controller)" }, [
    el("div", { class: "controller-head" }, [
      el("h3", {}, c.title + " Agent"),
      el("div", { class: "muted" }, c.role),
      el("div", { class: "tally", id: "tally-" + c.name }),
      el("div", { class: "last", id: "verdict-" + c.name }, "no calls inspected yet"),
    ]),
    el("ul", { class: "rules" }, c.rules.map((rule) => el("li", {}, rule))),
  ]));
}

// ---------- band panel ----------
function settingOf(agent) {
  if (!state) return "";
  if (agent === "Drummer") return `${state.drums.replaceAll("_", " ")} at ${state.tempo} bpm`;
  if (agent === "Bassist") return `${state.bass_movement} on ${state.bass_root}`;
  if (agent === "Lead") return state.lead;
  // An agent added later: describe it from its own pad and choice settings.
  const desc = padDescriptions.find((p) => agents[p.agent]?.agent === agent);
  if (!desc) return "";
  const held = desc.choice ? `${state[desc.choice.state]}, ` : "";
  return `${held}${desc.x.label} ${state[desc.x.state]}, ${desc.y.label} ${state[desc.y.state]}`;
}

function renderBand() {
  for (const a of agents) $("setting-" + a.agent).textContent = settingOf(a.agent);
  for (const pad of pads) pad.show(state);
  renderChoices();
}

/**
 * Buttons for a setting with a few named options, such as the lead sound.
 * A local recording, when loaded, appears as one more lead sound; it is a
 * setting of this page only, so the stage is not told about it.
 */
function renderChoices() {
  for (const { desc, agentIndex, box } of choices) {
    const isLeadSound = desc.state === "lead_sound";
    const usingSample = isLeadSound && audio.useSample;
    const pick = (value) => {
      audio.useSample = false;
      worker.postMessage({ type: "call", id: ++cueId, agent: agentIndex, call: { name: desc.tool, arguments: { [desc.arg]: value } } });
      renderChoices();
    };
    const buttons = desc.values.map((v) => el("button", {
      class: "chip" + (!usingSample && state?.[desc.state] === v ? " on" : ""),
      type: "button",
      "aria-pressed": String(!usingSample && state?.[desc.state] === v),
      onclick: () => pick(v),
    }, v.replaceAll("_", " ")));
    if (isLeadSound && audio.leadSample) {
      buttons.push(el("button", {
        class: "chip" + (usingSample ? " on" : ""),
        type: "button",
        "aria-pressed": String(usingSample),
        title: "your local recording",
        onclick: () => { audio.useSample = true; renderChoices(); },
      }, "sample"));
    }
    box.replaceChildren(el("span", { class: "muted" }, desc.label), ...buttons);
  }
}

/** Turns what the user asked for on a pad into tool calls, one per tool. */
function padCalls(desc, change) {
  const calls = new Map();
  for (const key of ["x", "y"]) {
    if (!(key in change)) continue;
    const axis = desc[key];
    if (!calls.has(axis.tool)) calls.set(axis.tool, { name: axis.tool, arguments: {} });
    calls.get(axis.tool).arguments[axis.arg] = change[key];
  }
  return [...calls.values()];
}

/** What an agent can be asked for, read from its tool declarations. */
function optionsOf(index) {
  return (briefs[index]?.tools ?? []).flatMap((t) =>
    Object.entries(t.function.parameters.properties).map(([name, p]) =>
      p.enum ? `${name}: ${p.enum.map((v) => v.replaceAll("_", " ")).join(", ")}`
        : p.type === "integer" ? `${name}: a number` : `${name}: ${p.description ?? "text"}`));
}

function buildBand() {
  pads = [];
  choices = [];
  const cards = agents.map((a, i) => {
    const desc = padDescriptions.find((p) => p.agent === i);
    const pad = desc && createPad(desc, a.agent, (change) => {
      for (const call of padCalls(desc, change)) worker.postMessage({ type: "call", id: ++cueId, agent: i, call });
    });
    if (pad) pads.push(pad);
    const box = desc?.choice && el("div", { class: "choices" });
    if (box) choices.push({ desc: desc.choice, agentIndex: i, box });
    return el("div", { class: "agent", id: "agent-" + a.agent, style: `--c:var(${COLORS[a.agent] ?? "--orchestra"})` }, [
      el("h3", {}, "Agent " + a.agent),
      el("div", { class: "muted" }, a.role),
      el("div", { class: "setting", id: "setting-" + a.agent }),
      el("div", { class: "last", id: "last-" + a.agent }, "touch the pad or send a cue"),
      el("div", { class: "meter", title: "model confidence of the last cue" }, el("div", { id: "meter-" + a.agent })),
      pad ? pad.element : el("div", { class: "options" }, optionsOf(i).map((o) => el("div", {}, o))),
      box || null,
    ]);
  });
  // The last tile adds one more agent to the orchestra.
  cards.push(el("button", { class: "agent add-agent", type: "button", id: "add-agent", onclick: addAgent }, [
    el("strong", {}, "+ Add agent"),
    el("span", { class: "muted" }, "One more orchestra section joins: its own timbre, note, detune and pulse, on the band's root and clock."),
  ]));
  $("band").replaceChildren(...controllerCards(), ...cards);
  signals.attach();
  renderControllers();
  renderBand();
}

function addAgent() {
  $("add-agent").disabled = true;
  worker.postMessage({ type: "spawn", id: ++cueId, kind: "drone" });
}

// ---------- audit log ----------
function logCue(result) {
  const lines = [];
  for (const turn of result.turns) {
    if (turn.error) lines.push(el("div", { class: "line" }, [el("strong", {}, turn.agent), " ", el("span", { class: "tag error" }, "error"), " " + turn.error]));
    let last = null;
    for (const attempt of turn.attempts) {
      if (!attempt.calls.length) {
        lines.push(el("div", { class: "line" }, [
          el("strong", {}, turn.agent), ` heard "${attempt.clause}" `,
          el("span", { class: "tag ignored" }, "ignored"), ` ${attempt.ignored} (confidence ${attempt.confidence.toFixed(2)})`,
          el("div", { class: "muted" }, `${turn.agent} understands ${optionsOf(agents.findIndex((a) => a.agent === turn.agent)).join("; ")}`),
        ]));
        last = { text: `ignored: ${attempt.ignored}`, confidence: attempt.confidence };
      }
      for (const c of attempt.calls) {
        const outcome = c.error ? `rejected by the stage: ${c.error}` : c.delivered ? c.result : "not applied";
        lines.push(el("div", { class: "line" }, [
          el("strong", {}, turn.agent), " ", el("code", {}, callText(c.call)),
          attempt.confidence !== null ? ` confidence ${attempt.confidence.toFixed(2)}, ${turn.latency_ms} ms`
            : result.source === "spawn" ? " asked by you" : " from the pad",
          ...c.verdicts.map((v) => el("span", { class: "tag " + v.verdict, title: v.detail }, `${v.module}: ${v.verdict}`)),
          c.result?.startsWith("no change") ? el("span", { class: "tag" }, "no change") : null,
          el("div", { class: "muted" }, c.verdicts
            .filter((v) => v.detail)
            .map((v) => (v.verdict === "modify" ? `changed to ${callText(c.delivered)}` : v.detail))
            .concat(outcome).join(" - ")),
        ]));
        last = { text: `${callText(c.delivered ?? c.call)}${c.delivered ? "" : " (blocked)"}`, confidence: attempt.confidence };
      }
    }
    if (last && $("agent-" + turn.agent)) {
      $("last-" + turn.agent).textContent = last.text;
      if (last.confidence !== null) $("meter-" + turn.agent).style.width = Math.round(last.confidence * 100) + "%";
      const card = $("agent-" + turn.agent);
      card.classList.remove("flash");
      void card.offsetWidth;
      card.classList.add("flash");
    }
  }
  if (!lines.length) {
    const names = briefs.map((b) => b.aliases?.[0]).filter(Boolean).join(", ");
    lines.push(el("div", { class: "line muted" }, `No agent was addressed. Name one of: ${names}.`));
  }
  const log = $("log");
  if (log.firstElementChild?.tagName === "P") log.replaceChildren();
  // The same cue sent again with the same outcome bumps a counter instead of adding a row.
  const signature = result.text.trim().toLowerCase() + "|" + JSON.stringify(result.turns.map((t) =>
    t.attempts.map((a) => [a.ignored, a.calls.map((c) => [c.call, c.verdicts.map((v) => v.verdict), c.result, c.error])])));
  const top = log.firstElementChild;
  // A drag on a pad sends many calls; keep one row per pad and tool, showing the latest.
  const padKey = result.source === "pad"
    ? result.text + ":" + result.turns[0].attempts[0].calls[0].call.name
    : null;
  if (padKey) {
    [...log.children].slice(0, 4).find((e) => e.dataset.pad === padKey)?.remove();
  } else if (top?.dataset.signature === signature) {
    top.dataset.count = Number(top.dataset.count) + 1;
    top.querySelector(".times").textContent = ` x${top.dataset.count}`;
    return;
  }
  const entry = el("div", { class: "entry", "data-signature": signature, "data-count": 1, "data-pad": padKey },
    [el("div", { class: "cue" }, [result.text, el("span", { class: "times muted" })]), ...lines]);
  log.prepend(entry);
  while (log.children.length > MAX_LOG_ENTRIES) log.lastElementChild.remove();
}

// ---------- worker messages ----------
worker.onmessage = ({ data }) => {
  if (data.type === "progress") {
    const pct = data.total ? Math.round((data.loaded / data.total) * 100) : 0;
    $("progress-bar").style.width = pct + "%";
    $("progress-text").textContent = data.label === "model"
      ? `Downloading Needle: ${(data.loaded / 1e6).toFixed(1)} / ${(data.total / 1e6).toFixed(1)} MB`
      : data.label === "engine" ? "Loading the Cactus engine" : "Starting the kernel and the band";
  } else if (data.type === "ready") {
    agents = data.setup.agents;
    controls = data.controls ?? [];
    briefs = data.briefs;
    state = data.state;
    $("chips").replaceChildren(...(data.setup.suggestions ?? []).map((s) =>
      el("button", { class: "chip", type: "button", onclick: () => sendCue(s) }, s)));
    $("start-card").hidden = true;
    for (const id of ["conductor", "band", "signal-legend", "roll-card", "log-card"]) $(id).hidden = false;
    audio.modelLabel = `Needle ${(data.modelBytes / 1e6).toFixed(0)} MB`;
    audio.metaBase = `${agents.length} agents - no server`;
    showMeta();
    padDescriptions = data.setup.pads ?? [];
    buildBand();
    setStatus("live", "live");
    worker.postMessage({ type: "render", bar: 0 });
  } else if (data.type === "bar") {
    audio.bars.set(data.bar, data.data);
    if (data.bar === 0 && !audio.playing) startPlayback();
  } else if (data.type === "cue-result") {
    const before = state;
    state = data.result.state;
    const added = data.result.added;
    if (added) {
      agents.push({ agent: added.agent, role: added.role });
      briefs.push(added.brief);
      if (added.pad) padDescriptions.push(added.pad);
      buildBand();
      $("agent-" + added.agent).scrollIntoView({ block: "nearest", behavior: "smooth" });
      audio.metaBase = audio.metaBase.replace(/\d+ agents/, `${agents.length} agents`);
      showMeta();
    } else if (data.result.source === "spawn") {
      $("add-agent").disabled = false;
    }
    renderBand();
    recordVerdicts(data.result);
    renderControllers();
    emitCue(data.result);
    emitVerdicts(data.result);
    emitChanges(before, state);
    logCue(data.result);
    // The next bar was rendered before the cue; ask again so the change lands on it.
    worker.postMessage({ type: "render", bar: audio.bar });
    $("send").disabled = false;
    setStatus("live", "live");
  } else if (data.type === "error") {
    console.error("kernel worker:", data.message, data.stack);
    setStatus("error", "bad");
    $("progress-text").textContent = data.message;
    $("send").disabled = false;
  }
};

function sendCue(text) {
  if (!text.trim() || $("send").disabled) return;
  $("send").disabled = true;
  setStatus("agents thinking", "busy");
  worker.postMessage({ type: "cue", id: ++cueId, text });
}

$("start").addEventListener("click", () => {
  $("start").disabled = true;
  $("progress").hidden = false;
  setStatus("loading", "busy");
  initAudio();
  worker.postMessage({ type: "init" });
});
$("cue-form").addEventListener("submit", (e) => {
  e.preventDefault();
  sendCue($("cue").value);
  $("cue").value = "";
});
$("volume").addEventListener("input", (e) => audio.synth?.setVolume(Number(e.target.value)));
$("toggle").addEventListener("click", () => {
  if (audio.playing) {
    audio.playing = false;
    audio.ctx.suspend();
    $("toggle").textContent = "Play";
  } else {
    startPlayback();
  }
});
drawRoll();
