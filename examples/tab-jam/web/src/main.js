// Tab Jam UI: sends cues to the kernel worker, plays what the stage renders
// with WebAudio, and shows what every agent and control did.

const $ = (id) => document.getElementById(id);
const worker = new Worker(new URL("worker.js?v=" + __BUILD_ID__, import.meta.url), { type: "module" });

const COLORS = { Drummer: "--drums", Bassist: "--bass", Lead: "--lead" };
const ROWS = ["lead", "bass", "hat", "snare", "kick"];
const ROW_COLOR = { lead: "--lead", bass: "--bass", hat: "--drums", snare: "--drums", kick: "--drums" };

const MAX_LOG_ENTRIES = 40;
let agents = [];
let briefs = [];
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

function setStatus(text, kind = "") {
  $("status").textContent = text;
  $("status").className = "pill " + kind;
}

// ---------- audio ----------
const audio = { ctx: null, master: null, noise: null, playing: false, nextBarTime: 0, bar: 0, bars: new Map(), last: null, now: null };

function initAudio() {
  const ctx = new AudioContext();
  const compressor = ctx.createDynamicsCompressor();
  const master = ctx.createGain();
  master.gain.value = 0.7;
  master.connect(compressor).connect(ctx.destination);
  const noise = ctx.createBuffer(1, ctx.sampleRate, ctx.sampleRate);
  const data = noise.getChannelData(0);
  for (let i = 0; i < data.length; i++) data[i] = Math.random() * 2 - 1;
  Object.assign(audio, { ctx, master, noise });
}

const hz = (midi) => 440 * 2 ** ((midi - 69) / 12);

function envelope(t, peak, attack, decay) {
  const g = audio.ctx.createGain();
  g.gain.setValueAtTime(0.0001, t);
  g.gain.exponentialRampToValueAtTime(peak, t + attack);
  g.gain.exponentialRampToValueAtTime(0.0001, t + attack + decay);
  g.connect(audio.master);
  return g;
}

function tone(type, freq, t, peak, attack, decay, filterHz) {
  const osc = audio.ctx.createOscillator();
  osc.type = type;
  osc.frequency.setValueAtTime(freq, t);
  let node = osc;
  if (filterHz) {
    const f = audio.ctx.createBiquadFilter();
    f.type = "lowpass";
    f.frequency.value = filterHz;
    node = osc.connect(f);
  }
  node.connect(envelope(t, peak, attack, decay));
  osc.start(t);
  osc.stop(t + attack + decay + 0.05);
  return osc;
}

function burst(t, peak, decay, highpassHz) {
  const src = audio.ctx.createBufferSource();
  src.buffer = audio.noise;
  const f = audio.ctx.createBiquadFilter();
  f.type = "highpass";
  f.frequency.value = highpassHz;
  src.connect(f).connect(envelope(t, peak, 0.002, decay));
  src.start(t);
  src.stop(t + decay + 0.05);
}

function playNote(n, t, step) {
  const v = n.vel / 127;
  const len = n.len * step;
  if (n.inst === "kick") {
    const osc = tone("sine", 150, t, v, 0.002, 0.22);
    osc.frequency.exponentialRampToValueAtTime(45, t + 0.12);
  } else if (n.inst === "snare") {
    burst(t, v * 0.6, 0.14, 1500);
    tone("triangle", 190, t, v * 0.3, 0.002, 0.08);
  } else if (n.inst === "hat") {
    burst(t, v * 0.25, 0.04, 7000);
  } else if (n.inst === "bass") {
    tone("sawtooth", hz(n.midi), t, v * 0.5, 0.01, Math.max(0.08, len * 0.9), 500);
  } else if (n.inst === "lead") {
    tone("square", hz(n.midi), t, v * 0.16, 0.01, Math.max(0.06, len * 0.95), 2400);
  }
}

function scheduleBar() {
  // If the worker is busy with a cue, repeat the previous bar instead of going silent.
  const data = audio.bars.get(audio.bar) ?? audio.last;
  if (!data) return;
  const step = 60 / data.tempo / 4;
  const start = audio.nextBarTime;
  for (const n of data.notes) playNote(n, start + n.step * step, step);
  audio.now = { data, start, duration: step * 16, bar: audio.bar };
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
    const r = ROWS.indexOf(n.inst);
    if (r < 0) continue;
    g.globalAlpha = 0.45 + 0.55 * (n.vel / 127);
    g.fillStyle = css.getPropertyValue(ROW_COLOR[n.inst]);
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

// ---------- band panel ----------
function settingOf(agent) {
  if (!state) return "";
  if (agent === "Drummer") return `${state.drums.replaceAll("_", " ")} at ${state.tempo} bpm`;
  if (agent === "Bassist") return `${state.bass_movement} on ${state.bass_root}`;
  return state.lead;
}

function renderBand() {
  for (const a of agents) $("setting-" + a.agent).textContent = settingOf(a.agent);
}

/** What an agent can be asked for, read from its tool declarations. */
function optionsOf(index) {
  return (briefs[index]?.tools ?? []).flatMap((t) =>
    Object.entries(t.function.parameters.properties).map(([name, p]) =>
      p.enum ? `${name}: ${p.enum.map((v) => v.replaceAll("_", " ")).join(", ")}`
        : p.type === "integer" ? `${name}: a number` : `${name}: ${p.description ?? "text"}`));
}

function buildBand() {
  $("band").replaceChildren(...agents.map((a, i) =>
    el("div", { class: "agent", id: "agent-" + a.agent, style: `--c:var(${COLORS[a.agent] ?? "--accent"})` }, [
      el("h3", {}, a.agent),
      el("div", { class: "muted" }, a.role),
      el("div", { class: "setting", id: "setting-" + a.agent }),
      el("div", { class: "last", id: "last-" + a.agent }, "waiting for a cue"),
      el("div", { class: "meter", title: "model confidence of the last call" }, el("div", { id: "meter-" + a.agent })),
      el("div", { class: "options" }, optionsOf(i).map((o) => el("div", {}, o))),
    ])));
  renderBand();
}

// ---------- audit log ----------
const callText = (c) => `${c.name}(${Object.entries(c.arguments).map(([k, v]) => `${k}: ${v}`).join(", ")})`;

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
          ` confidence ${attempt.confidence.toFixed(2)}, ${turn.latency_ms} ms`,
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
    if (last) {
      $("last-" + turn.agent).textContent = last.text;
      $("meter-" + turn.agent).style.width = Math.round(last.confidence * 100) + "%";
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
  if (top?.dataset.signature === signature) {
    top.dataset.count = Number(top.dataset.count) + 1;
    top.querySelector(".times").textContent = ` x${top.dataset.count}`;
    return;
  }
  const entry = el("div", { class: "entry", "data-signature": signature, "data-count": 1 },
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
    briefs = data.briefs;
    state = data.state;
    $("chips").replaceChildren(...(data.setup.suggestions ?? []).map((s) =>
      el("button", { class: "chip", type: "button", onclick: () => sendCue(s) }, s)));
    $("start-card").hidden = true;
    for (const id of ["conductor", "band", "roll-card", "log-card"]) $(id).hidden = false;
    $("meta").textContent = `Needle ${(data.modelBytes / 1e6).toFixed(0)} MB - ${agents.length} agents - ${data.setup.key} - no server`;
    buildBand();
    setStatus("live", "live");
    worker.postMessage({ type: "render", bar: 0 });
  } else if (data.type === "bar") {
    audio.bars.set(data.bar, data.data);
    if (data.bar === 0 && !audio.playing) startPlayback();
  } else if (data.type === "cue-result") {
    state = data.result.state;
    renderBand();
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
