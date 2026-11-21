// The duck soccer page: the match is drawn here from the frames the kernel
// sends back; the brains, the bodies and the referee run in soccer-worker.js.
import { connectBrains } from "./brain-link.js";
import { SCHEMES, drawDuck as drawDuckOn, portrait } from "./duck-art.js";

const $ = (id) => document.getElementById(id);
const css = (v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
const canvas = $("room"), ctx = canvas.getContext("2d");
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const TEAM = { Sky: "#2fa8c4", Sun: "#f2a11d" };
const TICK_MS = 100;

let setup = null, frame = null, prevFrame = null, frameAt = 0;
let scale = 1, W = 3.4, H = 2.4, selected = -1, speed = 2;
let goalFlash = null;   // { team, until } while a goal is being celebrated
// Room above the far touchline, so a duck standing there is drawn whole.
const TOP_M = 0.42;
const X = (x) => x * scale, Y = (y) => (H - y + TOP_M) * scale;
const view = () => ({ ctx, scale, X, Y, css, reduced, tall: 0.42 });

$("logo").append(portrait(SCHEMES[0], {}, 56));

const brains = connectBrains({
  worker: new Worker(new URL("./soccer-worker.js", import.meta.url), { type: "module" }),
  statusEl: $("status"),
  logEl: $("log"),
  onReady(kernelSetup, controls) {
    setup = kernelSetup;
    W = setup.pitch.w; H = setup.pitch.h;
    $("main").hidden = false;
    for (const text of setup.suggestions) {
      const b = document.createElement("button");
      b.type = "button"; b.className = "chip-btn"; b.textContent = text;
      b.addEventListener("click", () => brains.send(text, "coach"));
      $("suggest").append(b);
    }
    const referee = controls[0];
    if (referee) {
      $("refTitle").textContent = referee.title;
      for (const rule of referee.rules) $("rules").append(Object.assign(document.createElement("li"), { textContent: rule }));
    }
    resize();
    loop();
  },
  onFrame(next) {
    const goal = next.events.find((e) => e.goal);
    // After a goal the ducks are back at their spots: do not draw them sliding there.
    prevFrame = goal ? null : frame;
    frame = next;
    frameAt = performance.now();
    if (goal) goalFlash = { team: goal.goal, until: performance.now() + 2200 };
    for (const u of next.urges) askOnce(u);
    for (const e of next.events) {
      if (e.goal) brains.logLine(e.text, "kick-off from the centre", "goal");
      else if (e.agent === "Referee" || e.text.startsWith("the referee")) brains.logLine(`${e.agent}: ${e.text}`, "", "ref");
    }
    $("scoreSky").textContent = next.score.Sky;
    $("scoreSun").textContent = next.score.Sun;
    const s = Math.floor(next.t);
    $("clock").textContent = next.whistle_s > 0 ? "kick-off…" : `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;
    updateCards();
  },
  onCue: () => updateCards(),
});
window.__brainMs = brains.brainMs; // read by the browser check
window.__frame = () => frame;

// One urge per duck at a time: a duck still waiting for its brain does not ask again.
const asking = new Set();
function askOnce(u) {
  const name = setup.agents[u.agent].agent;
  if (asking.has(name)) return;
  asking.add(name);
  setTimeout(() => asking.delete(name), 1500);
  brains.send(u.text, u.why);
}

$("cueForm").addEventListener("submit", (e) => {
  e.preventDefault();
  brains.send($("cue").value.trim(), "coach");
  $("cue").value = "";
});
for (const b of document.querySelectorAll(".speed button")) {
  b.addEventListener("click", () => {
    speed = Number(b.dataset.speed);
    document.querySelectorAll(".speed button").forEach((o) => o.classList.toggle("on", o === b));
  });
}

let tick = 0, lastRender = 0;
function loop() {
  const now = performance.now();
  // The bodies keep their own clock; `speed` ticks of it per 100 ms of the page's.
  if (now - lastRender >= TICK_MS && brains.render(tick, speed)) {
    lastRender = now;
    tick += speed;
  }
  draw();
  requestAnimationFrame(loop);
}

function rr(x, y, w, h, r) { ctx.beginPath(); ctx.roundRect(x, y, w, h, r); }

function drawPitch() {
  const s = scale, half = setup.pitch.goal_half, top = Y(H), bottom = Y(0);
  // The stand behind the far touchline.
  ctx.fillStyle = css("--bg2"); ctx.fillRect(0, 0, canvas.width, top);
  // Mown stripes.
  const stripes = 8;
  for (let k = 0; k < stripes; k++) {
    ctx.fillStyle = k % 2 ? "#4c9a52" : "#55a55b";
    ctx.fillRect((canvas.width / stripes) * k, top, canvas.width / stripes + 1, bottom - top);
  }
  ctx.strokeStyle = "rgba(255,255,255,.85)"; ctx.lineWidth = s * 0.018;
  const m = s * 0.06;
  ctx.strokeRect(m, top + m, canvas.width - 2 * m, bottom - top - 2 * m);
  ctx.beginPath(); ctx.moveTo(X(W / 2), top + m); ctx.lineTo(X(W / 2), bottom - m); ctx.stroke();
  ctx.beginPath(); ctx.arc(X(W / 2), Y(H / 2), s * 0.38, 0, 7); ctx.stroke();
  ctx.fillStyle = "rgba(255,255,255,.85)"; ctx.beginPath(); ctx.arc(X(W / 2), Y(H / 2), s * 0.025, 0, 7); ctx.fill();
  // Goal areas and the goals themselves: Sky defends the left, Sun the right.
  for (const [team, x0, dir] of [["Sky", 0, 1], ["Sun", W, -1]]) {
    ctx.strokeStyle = "rgba(255,255,255,.85)";
    ctx.strokeRect(Math.min(X(x0) + dir * m, X(x0 + dir * 0.55)), Y(H / 2 + half + 0.2), Math.abs(X(0.55) - m), (2 * half + 0.4) * s);
    ctx.fillStyle = TEAM[team]; ctx.globalAlpha = 0.9;
    rr(Math.min(X(x0), X(x0 + dir * 0.1)), Y(H / 2 + half), 0.1 * s, 2 * half * s, s * 0.03); ctx.fill();
    ctx.globalAlpha = 1;
    // Net.
    ctx.strokeStyle = "rgba(255,255,255,.6)"; ctx.lineWidth = s * 0.006;
    for (let k = 1; k < 8; k++) {
      const y = Y(H / 2 + half) + ((2 * half * s) / 8) * k;
      ctx.beginPath(); ctx.moveTo(Math.min(X(x0), X(x0 + dir * 0.1)), y); ctx.lineTo(Math.max(X(x0), X(x0 + dir * 0.1)), y); ctx.stroke();
    }
    ctx.lineWidth = s * 0.018;
  }
}

function draw() {
  if (!frame) return;
  // Between two body ticks the page draws the in-between pose.
  const u = prevFrame && !reduced ? Math.min((performance.now() - frameAt) / TICK_MS, 1) : 1;
  const mix = (a, b) => a + (b - a) * u;
  const turn = (a, b) => a + Math.atan2(Math.sin(b - a), Math.cos(b - a)) * u;
  const t = prevFrame ? mix(prevFrame.t, frame.t) : frame.t;
  drawPitch();

  const sprites = frame.ducks.map((now, i) => {
    const was = prevFrame ? prevFrame.ducks[i] : now;
    return { i, d: { ...now, x: mix(was.x, now.x), y: mix(was.y, now.y), heading: turn(was.heading, now.heading) } };
  });
  const ball = prevFrame ? [mix(prevFrame.ball[0], frame.ball[0]), mix(prevFrame.ball[1], frame.ball[1])] : frame.ball;

  // Everything standing on the pitch, far to near.
  const things = [...sprites.map((sp) => ({ y: sp.d.y, sp })), { y: ball[1], ball }];
  things.sort((a, b) => b.y - a.y);
  for (const thing of things) {
    if (thing.ball) {
      const r = scale * 0.055;
      ctx.fillStyle = "rgba(0,0,0,.22)"; ctx.beginPath(); ctx.ellipse(X(ball[0]), Y(ball[1]) + r * 0.6, r, r * 0.4, 0, 0, 7); ctx.fill();
      ctx.fillStyle = "#fff"; ctx.beginPath(); ctx.arc(X(ball[0]), Y(ball[1]) - r * 0.3, r, 0, 7); ctx.fill();
      ctx.strokeStyle = "#22252e"; ctx.lineWidth = scale * 0.008; ctx.stroke();
      ctx.fillStyle = "#22252e"; ctx.beginPath(); ctx.arc(X(ball[0]), Y(ball[1]) - r * 0.3, r * 0.38, 0, 7); ctx.fill();
      continue;
    }
    const { i, d } = thing.sp;
    drawDuckOn(view(), i, d.x, d.y, d.heading, TEAM[d.team], {
      policy: d.policy, state: d.action, quack: false, scheme: SCHEMES[i % SCHEMES.length], walking: d.policy === "walk",
    }, t, i === selected);
  }
  ctx.font = `600 ${scale * 0.062}px ui-rounded, system-ui`; ctx.textAlign = "center"; ctx.textBaseline = "middle";
  for (const { i, d } of sprites) {
    const w = ctx.measureText(d.agent).width + scale * 0.07, hgt = scale * 0.095, cx = X(d.x), cy = Y(d.y) + scale * 0.13;
    ctx.fillStyle = css("--card"); ctx.globalAlpha = 0.9; rr(cx - w / 2, cy - hgt / 2, w, hgt, hgt / 2); ctx.fill(); ctx.globalAlpha = 1;
    ctx.strokeStyle = TEAM[d.team]; ctx.lineWidth = scale * 0.01; ctx.stroke();
    ctx.fillStyle = css("--ink"); ctx.fillText(d.agent, cx, cy + scale * 0.004);
  }
  if (goalFlash && performance.now() < goalFlash.until) {
    const mid = Y(H / 2);
    ctx.fillStyle = "rgba(0,0,0,.35)"; ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = TEAM[goalFlash.team]; ctx.font = `800 ${scale * 0.5}px ui-rounded, system-ui`;
    ctx.fillText("GOAL!", canvas.width / 2, mid);
    ctx.fillStyle = "#fff"; ctx.font = `600 ${scale * 0.13}px ui-rounded, system-ui`;
    ctx.fillText(`${goalFlash.team} scores`, canvas.width / 2, mid + scale * 0.36);
  }
  ctx.textAlign = "start"; ctx.textBaseline = "alphabetic";
}

let cardsKey = "";
function updateCards() {
  if (!frame) return;
  const key = JSON.stringify([selected, frame.ducks.map((d) => [d.action, d.policy, Math.round(d.energy * 20), brains.lastBrain[d.agent]])]);
  if (key === cardsKey) return;
  cardsKey = key;
  const el = (tag, cls, text) => { const e = document.createElement(tag); if (cls) e.className = cls; if (text != null) e.textContent = text; return e; };
  const box = $("ducks");
  box.textContent = "";
  frame.ducks.forEach((d, i) => {
    const card = el("article", "card duck" + (i === selected ? " sel" : ""));
    card.style.setProperty("--c", TEAM[d.team]);
    const head = el("header");
    const av = el("div", "avatar"); av.append(portrait(SCHEMES[i % SCHEMES.length], {}, 64));
    const who = el("div"); who.append(el("h2", "", d.agent), el("div", "role", `${d.team} team`));
    const chips = el("div", "chips");
    chips.append(el("span", "chip", d.action.replace("_", " ")), el("span", "chip body " + d.policy, "body: " + d.policy));
    head.append(av, who, chips);
    const b = brains.lastBrain[d.agent];
    const energy = el("div", "energy"); const bar = el("div", "bar"); const fill = el("i");
    fill.style.width = Math.round(d.energy * 100) + "%"; fill.className = d.energy < 0.2 ? "low" : d.energy < 0.5 ? "mid" : "";
    bar.append(fill); energy.append("energy", bar, Math.round(d.energy * 100) + "%");
    card.append(head, el("div", "thought", b ? `${b.text}${b.verdict ? "  ·  " + b.verdict : ""}` : ""), energy);
    card.addEventListener("click", () => { selected = selected === i ? -1 : i; updateCards(); });
    box.append(card);
  });
}

function resize() {
  if (!setup) return;
  const cssW = canvas.clientWidth, dpr = window.devicePixelRatio || 1;
  canvas.style.height = (cssW * (H + TOP_M)) / W + "px";
  canvas.width = cssW * dpr; canvas.height = ((cssW * (H + TOP_M)) / W) * dpr;
  scale = canvas.width / W;
  draw();
}
addEventListener("resize", resize);
