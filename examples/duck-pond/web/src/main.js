// The duck pond page: the bodies are drawn here from the frames the kernel
// sends back; the brains and the bodies run in worker.js.
import { connectBrains } from "./brain-link.js";
import { COLORS, SCHEMES, drawDuck as drawDuckOn, portrait } from "./duck-art.js";

const $ = (id) => document.getElementById(id);
const css = (v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
const canvas = $("room"), ctx = canvas.getContext("2d");
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const data = { room: null };
let scale = 1, W = 4, H = 3;
let selected = -1;
const headings = [];

let setup = null, frame = null;
const worker = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });

  // ---------- drawing ----------
  const X = (x) => x * scale, Y = (y) => (H - y) * scale;



  function rr(x, y, w, h, r) { ctx.beginPath(); ctx.roundRect(x, y, w, h, r); }

  function drawRoom() {
    const s = scale;
    ctx.fillStyle = css("--floor"); ctx.fillRect(0, 0, canvas.width, canvas.height);
    // Planks.
    ctx.strokeStyle = css("--plank"); ctx.lineWidth = Math.max(1, s * 0.006);
    for (let y = 0.25, k = 0; y < H; y += 0.25, k++) {
      ctx.beginPath(); ctx.moveTo(0, Y(y)); ctx.lineTo(canvas.width, Y(y)); ctx.stroke();
      for (let x = (k % 2 ? 0.5 : 1.1); x < W; x += 1.2) { ctx.beginPath(); ctx.moveTo(X(x), Y(y)); ctx.lineTo(X(x), Y(y - 0.25)); ctx.stroke(); }
    }
    // Cushion.
    const [cx, cy] = data.room.cushion;
    ctx.fillStyle = css("--rug"); ctx.beginPath(); ctx.arc(X(cx), Y(cy), s * 0.3, 0, 7); ctx.fill();
    ctx.fillStyle = css("--rug2"); ctx.beginPath(); ctx.arc(X(cx), Y(cy), s * 0.2, 0, 7); ctx.fill();
    // Table top, see-through: the ducks walk under it.
    const legs = data.room.obstacles.filter((o) => o.name === "table leg").map((o) => o.circle);
    if (legs.length === 4) {
      const xs = legs.map((l) => l[0]), ys = legs.map((l) => l[1]);
      ctx.save(); ctx.globalAlpha = 0.22; ctx.fillStyle = css("--furn");
      rr(X(Math.min(...xs)) - s * 0.08, Y(Math.max(...ys)) - s * 0.08, (Math.max(...xs) - Math.min(...xs)) * s + s * 0.16, (Math.max(...ys) - Math.min(...ys)) * s + s * 0.16, s * 0.06);
      ctx.fill(); ctx.restore();
    }
    for (const o of data.room.obstacles) {
      ctx.save();
      ctx.shadowColor = "rgba(0,0,0,.28)"; ctx.shadowBlur = s * 0.05; ctx.shadowOffsetY = s * 0.015;
      if (o.rect) {
        const [x0, y0, x1, y1] = o.rect;
        ctx.fillStyle = css("--furn"); rr(X(x0), Y(y1), (x1 - x0) * s, (y1 - y0) * s, s * 0.07); ctx.fill();
        ctx.shadowColor = "transparent"; ctx.fillStyle = css("--furn2");
        rr(X(x0) + s * 0.06, Y(y1) + s * 0.06, (x1 - x0) * s - s * 0.12, (y1 - y0) * s - s * 0.12, s * 0.05); ctx.fill();
      } else {
        const [x, y, r] = o.circle;
        ctx.fillStyle = o.name === "plant pot" ? "#c9764f" : css("--furn2");
        ctx.beginPath(); ctx.arc(X(x), Y(y), r * s, 0, 7); ctx.fill();
        if (o.name === "plant pot") {
          ctx.shadowColor = "transparent"; ctx.fillStyle = "#3f9b5f";
          for (let a = 0; a < 6; a++) { ctx.beginPath(); ctx.ellipse(X(x) + Math.cos(a) * r * s * 0.6, Y(y) + Math.sin(a) * r * s * 0.6, r * s * 0.55, r * s * 0.28, a, 0, 7); ctx.fill(); }
        }
      }
      ctx.restore();
    }
    ctx.strokeStyle = css("--wall"); ctx.lineWidth = s * 0.05; ctx.strokeRect(0, 0, canvas.width, canvas.height);
  }


const drawDuck = (...args) => drawDuckOn({ ctx, scale, X, Y, css, reduced }, ...args);

// ---------- the kernel ----------
const brains = connectBrains({
  worker,
  statusEl: $("status"),
  logEl: $("log"),
  onReady(kernelSetup) {
    setup = kernelSetup;
    data.room = setup.room;
    $("main").hidden = false;
    suggestions();
    resize();
    loop();
  },
  onFrame(next) {
    prevFrame = frame;
    frame = next;
    frameAt = performance.now();
    onFrame();
  },
  onCue: () => updateCards(),
});
const { send, logLine, lastBrain } = brains;
window.__brainMs = brains.brainMs; // read by the browser check

// ---------- the page ----------
function suggestions() {
  const box = $("suggest");
  for (const text of setup.suggestions) {
    const b = document.createElement("button");
    b.type = "button"; b.className = "chip-btn"; b.textContent = text;
    b.addEventListener("click", () => send(text, "you"));
    box.append(b);
  }
}

$("cueForm").addEventListener("submit", (e) => {
  e.preventDefault();
  const input = $("cue");
  send(input.value.trim(), "you");
  input.value = "";
});

// Idle ducks ask their brains for something to do; the brains answer through the safety control.
let frameTick = 0;
let lastRender = 0;
let prevFrame = null, frameAt = 0;
const TICK_MS = 100;
function loop() {
  const now = performance.now();
  // The body keeps its own clock: one tick per 100 ms, also while a brain is thinking.
  if (now - lastRender >= TICK_MS && brains.render(frameTick)) {
    lastRender = now;
    frameTick++;
  }
  draw();
  requestAnimationFrame(loop);
}

function onFrame() {
  for (const u of frame.urges || []) queueUrge(u);
  for (const e of frame.events || []) feedEvent(e);
  updateCards();
  if (frame.language) updateLanguage(frame.language);
}

const urgeAsked = new Set();
function queueUrge(u) {
  const name = setup.agents[u.agent].agent;
  // One pending urge per duck: a duck that is still waiting does not ask again.
  if (urgeAsked.has(name)) return;
  urgeAsked.add(name);
  setTimeout(() => urgeAsked.delete(name), 6000);
  send(u.text, u.why);
}

const recentEvents = [];
const lastSay = {};       // duck name -> { word, t } for the speech bubble
function feedEvent(e) {
  recentEvents.push(e);
  if (recentEvents.length > 8) recentEvents.shift();
  if (e.text.startsWith("quacks")) logLine(`${e.agent} quacks`, e.text.replace(/^quacks: /, "because "));
  if (e.text.startsWith("says ")) {
    const word = e.text.split(" ")[1];
    lastSay[e.agent] = { word, t: frame.t };
    logLine(`${e.agent} says "${word}"`, e.text.replace(/^says \S+ /, ""));
  }
}

let langKey = "";
function updateLanguage(lang) {
  const key = JSON.stringify(lang);
  if (key === langKey) return;
  langKey = key;
  const box = $("lang");
  box.textContent = "";
  const table = document.createElement("table");
  const head = table.insertRow();
  head.append(Object.assign(document.createElement("th"), { textContent: "word" }));
  for (const d of lang.ducks) head.append(Object.assign(document.createElement("th"), { textContent: d.agent }));
  lang.words.forEach((w, s) => {
    const row = table.insertRow();
    row.append(Object.assign(document.createElement("td"), { textContent: `"${w}"` }));
    for (const d of lang.ducks) {
      const meaning = d.means[s];
      const td = document.createElement("td");
      td.textContent = meaning || "—";
      if (!meaning) td.className = "none";
      row.append(td);
    }
  });
  box.append(table);
  const agree = document.createElement("div");
  agree.className = "agree";
  agree.textContent = lang.agreement === null
    ? "No word is shared yet: the ducks are still babbling."
    : `The ducks agree on ${Math.round(lang.agreement * 100)}% of the words they know.`;
  box.append(agree);
}

function draw() {
  if (!frame) return;
  // Between two body ticks the page draws the in-between pose, so motion is smooth at screen rate.
  const u = prevFrame && !reduced ? Math.min((performance.now() - frameAt) / TICK_MS, 1) : 1;
  const mix = (a, b) => a + (b - a) * u;
  const turn = (a, b) => a + Math.atan2(Math.sin(b - a), Math.cos(b - a)) * u;
  const f = prevFrame ? mix(prevFrame.t, frame.t) : frame.t;
  drawRoom();
  const ball = prevFrame ? [mix(prevFrame.ball[0], frame.ball[0]), mix(prevFrame.ball[1], frame.ball[1])] : frame.ball;
  ctx.save(); ctx.shadowColor = "rgba(0,0,0,.3)"; ctx.shadowBlur = scale * 0.03;
  ctx.fillStyle = "#f6883a"; ctx.beginPath(); ctx.arc(X(ball[0]), Y(ball[1]), scale * 0.05, 0, 7); ctx.fill(); ctx.restore();
  const sprites = frame.ducks.map((now, i) => {
    const was = prevFrame ? prevFrame.ducks[i] : now;
    const d = { ...now, x: mix(was.x, now.x), y: mix(was.y, now.y), heading: turn(was.heading, now.heading) };
    return { i, d, color: COLORS[i % COLORS.length], y: d.y };
  });
  sprites.sort((a, b) => b.y - a.y);
  for (const sp of sprites) {
    const d = sp.d;
    headings[sp.i] = d.heading;
    drawDuck(sp.i, d.x, d.y, d.heading, sp.color, {
      policy: d.policy, state: d.action, quack: d.action === "quack", scheme: SCHEMES[sp.i], walking: d.policy === "walk",
    }, f, sp.i === selected);
  }
  // Speech bubbles: the word a duck said in the last moments.
  ctx.font = `700 ${scale * 0.07}px ui-rounded, system-ui`; ctx.textAlign = "center"; ctx.textBaseline = "middle";
  for (const sp of sprites) {
    const say = lastSay[sp.d.agent];
    if (!say || f - say.t > 2.5) continue;
    const text = `"${say.word}"`, w = ctx.measureText(text).width + scale * 0.08, hgt = scale * 0.11;
    const cx = X(sp.d.x), cy = Y(sp.d.y) - scale * 0.27 - (f - say.t) * scale * 0.02;
    ctx.fillStyle = css("--card"); rr(cx - w / 2, cy - hgt / 2, w, hgt, hgt / 2); ctx.fill();
    ctx.strokeStyle = sp.color; ctx.lineWidth = scale * 0.01; ctx.stroke();
    ctx.fillStyle = css("--ink"); ctx.fillText(text, cx, cy + scale * 0.004);
  }
  ctx.font = `600 ${scale * 0.062}px ui-rounded, system-ui`; ctx.textAlign = "center"; ctx.textBaseline = "middle";
  for (const sp of sprites) {
    const w = ctx.measureText(sp.d.agent).width + scale * 0.07, hgt = scale * 0.095, cx = X(sp.d.x), cy = Y(sp.d.y) + scale * 0.13;
    ctx.fillStyle = css("--card"); ctx.globalAlpha = 0.88; rr(cx - w / 2, cy - hgt / 2, w, hgt, hgt / 2); ctx.fill(); ctx.globalAlpha = 1;
    ctx.strokeStyle = sp.color; ctx.lineWidth = scale * 0.008; ctx.stroke();
    ctx.fillStyle = css("--ink"); ctx.fillText(sp.d.agent, cx, cy + scale * 0.004);
  }
  ctx.textAlign = "start"; ctx.textBaseline = "alphabetic";
}

let cardsKey = "";
function updateCards() {
  if (!frame) return;
  const key = JSON.stringify([selected, frame.ducks.map((d) => [d.action, d.place, d.policy, Math.round(d.energy * 100), lastBrain[d.agent]])]);
  if (key === cardsKey) return;
  cardsKey = key;
  const box = $("ducks");
  box.textContent = "";
  frame.ducks.forEach((d, i) => {
    const card = document.createElement("article");
    card.className = "card duck" + (i === selected ? " sel" : "");
    card.style.setProperty("--c", COLORS[i % COLORS.length]);
    const head = document.createElement("header");
    const av = document.createElement("div"); av.className = "avatar";
    av.append(portrait(SCHEMES[i % SCHEMES.length], { sit: d.policy === "sit", beak: d.action === "quack" ? 0.18 : 0 }, 64));
    const who = document.createElement("div");
    const h = document.createElement("h2"); h.textContent = d.agent;
    const role = document.createElement("div"); role.className = "role"; role.textContent = setup.agents[i].role;
    who.append(h, role);
    const chips = document.createElement("div"); chips.className = "chips";
    const doing = document.createElement("span"); doing.className = "chip"; doing.textContent = d.action.replace("_", " ") + (d.place ? ` · ${d.place}` : "");
    const body = document.createElement("span"); body.className = "chip body " + d.policy; body.textContent = "body: " + d.policy;
    chips.append(doing, body);
    head.append(av, who, chips);
    const brain = document.createElement("div"); brain.className = "thought";
    const b = lastBrain[d.agent];
    brain.textContent = b ? `${b.text}${b.verdict ? "  ·  " + b.verdict : ""}` : "";
    const energy = document.createElement("div"); energy.className = "energy";
    const bar = document.createElement("div"); bar.className = "bar";
    const fill = document.createElement("i"); fill.style.width = Math.round(d.energy * 100) + "%";
    fill.className = d.energy < 0.2 ? "low" : d.energy < 0.5 ? "mid" : "";
    bar.append(fill); energy.append("energy", bar, Math.round(d.energy * 100) + "%");
    card.append(head, brain, energy);
    card.addEventListener("click", () => { selected = selected === i ? -1 : i; updateCards(); draw(); });
    box.append(card);
  });
}

function resize() {
  if (!data.room) return;
  const cssW = canvas.clientWidth, dpr = window.devicePixelRatio || 1;
  canvas.style.height = (cssW * data.room.h) / data.room.w + "px";
  canvas.width = cssW * dpr; canvas.height = ((cssW * data.room.h) / data.room.w) * dpr;
  scale = canvas.width / data.room.w;
  W = data.room.w; H = data.room.h;
  draw();
}
addEventListener("resize", resize);
