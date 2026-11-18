// The duck pond page. The bodies tick in worker.js and the brains think in
// brain.js: two workers, so the animation never waits for the model. This page
// passes a cue from the brains to the bodies, and draws at the display's rate,
// easing from one body frame to the next.
const $ = (id) => document.getElementById(id);
const css = (v) => getComputedStyle(document.documentElement).getPropertyValue(v).trim();
const canvas = $("room"), ctx = canvas.getContext("2d");
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const data = { room: null };
let scale = 1, W = 4, H = 3;
let selected = -1;
const headings = [];

let setup = null, frame = null;
let cuePending = false, cueId = 0;
const queue = [];          // cues waiting for the brain: { text, why, urge, at }
const lastBrain = {};     // duck name -> last thing its brain did, for the cards
let thinking = new Set(); // ducks whose brain is working on a cue right now
const body = new Worker(new URL("./worker.js", import.meta.url), { type: "module" });
const brain = new Worker(new URL("./brain.js", import.meta.url), { type: "module" });
let briefs = [], bodyReady = false, brainReady = false;
// Read by the browser test: how many frames came and how long the page ever went undrawn.
window.__pond = { frames: 0, draws: 0, maxGapMs: 0 };

  const SCHEMES = [
    { shell: "#8fd6e3", beak: "#f2682a", eye: "#f5a623", foot: "#f2682a", sole: "#f5b73a", accent: "#3fb6cc" },
    { shell: "#6c6e75", beak: "#f4cd3c", eye: "#8b6fd0", foot: "#f4cd3c", sole: "#8b6fd0", accent: "#e0b21f" },
    { shell: "#ece5d6", beak: "#f2682a", eye: "#f59a1f", foot: "#f59a1f", sole: "#f2682a", accent: "#f2682a" },
    { shell: "#bda8dc", beak: "#f4dc4a", eye: "#7fd0de", foot: "#f4dc4a", sole: "#8b6fd0", accent: "#9a7fd0" },
  ];
  const COLORS = SCHEMES.map((c) => c.accent);
  const SERVO = "#1b1b20";
  // One model of a MicroDuck, side view facing right, drawn on any canvas: the
  // map, the cards and the header use it. Origin is the ground under the body,
  // `u` is one model unit in pixels (the duck is 84 units tall).
  function drawModel(g, c, u, pose) {
    const P = (v) => v * u;
    const box = (x, y, w, h, r) => { g.beginPath(); g.roundRect(P(x), P(y), P(w), P(h), Array.isArray(r) ? r.map(P) : P(r)); };
    const dot = (x, y, r) => { g.beginPath(); g.arc(P(x), P(y), P(r), 0, 7); };
    const sit = !!pose.sit, phase = pose.phase || 0;
    const B = (sit ? 18 : 0) - (pose.walking ? Math.abs(Math.sin(phase)) * 1.4 : 0);
    g.save();
    g.lineCap = "round"; g.lineJoin = "round";
    // Lying on its side after a fall.
    if (pose.down) { g.translate(P(-34), P(-13)); g.rotate(Math.PI / 2); }

    const leg = (dx, ph) => {
      let fx = dx, lift = 0;
      if (pose.walking && !sit) { fx += Math.sin(ph) * 6; lift = Math.max(0, Math.cos(ph)) * 3.5; }
      if (sit) fx = dx + 13;
      g.strokeStyle = SERVO; g.lineWidth = P(6.5);
      g.beginPath(); g.moveTo(P(dx), P(-28 + B)); g.lineTo(P(sit ? dx + 9 : dx + 1), P(sit ? -12 : -17 - lift)); g.lineTo(P(fx - 3), P(-9 - lift)); g.stroke();
      g.fillStyle = "#8f9198"; dot(sit ? dx + 9 : dx + 1, sit ? -12 : -17 - lift, 1.9); g.fill();
      g.fillStyle = c.sole; box(fx - 10, -3.4 - lift, 21, 3.4, 1.7); g.fill();
      g.fillStyle = c.foot; box(fx - 10, -10.5 - lift, 18.5, 7.6, [4.5, 6, 1, 1]); g.fill();
      g.fillStyle = "rgba(0,0,0,.28)"; dot(fx - 5, -6 - lift, 0.8); g.fill(); dot(fx - 2, -6 - lift, 0.8); g.fill(); dot(fx - 3.5, -8 - lift, 0.8); g.fill();
    };
    leg(-8, phase + Math.PI);
    leg(6, phase);

    // Hip servos and the thigh shield.
    g.fillStyle = SERVO; box(-10, -37 + B, 22, 12, 3); g.fill();
    g.fillStyle = "#8f9198"; dot(-2.5, -31 + B, 2.7); g.fill(); dot(6, -31 + B, 2.7); g.fill();
    g.fillStyle = "#c3c5cb"; dot(-2.5, -31 + B, 1.1); g.fill(); dot(6, -31 + B, 1.1); g.fill();
    g.fillStyle = c.shell; g.strokeStyle = c.shell; g.lineWidth = P(4);
    g.beginPath(); g.moveTo(P(-17), P(-33 + B)); g.lineTo(P(-6), P(-36 + B)); g.lineTo(P(-3), P(-25 + B)); g.lineTo(P(-13), P(-20 + B)); g.closePath(); g.fill(); g.stroke();
    g.fillStyle = "rgba(255,255,255,.75)"; dot(-13.5, -30 + B, 0.9); g.fill(); dot(-7, -27 + B, 0.9); g.fill();

    // Body.
    g.fillStyle = c.shell; box(-14, -51 + B, 27, 16, 6.5); g.fill();
    g.fillStyle = "rgba(0,0,0,.09)"; box(-14, -43 + B, 27, 8, [0, 0, 6.5, 6.5]); g.fill();
    g.fillStyle = "rgba(255,255,255,.3)"; box(-10, -49.5 + B, 14, 2.6, 1.3); g.fill();
    g.fillStyle = "rgba(0,0,0,.35)"; for (const [x, y] of [[6, -45], [9, -43], [6.5, -41]]) { dot(x, y + B, 0.85); g.fill(); }

    // Neck: two servo segments.
    g.strokeStyle = SERVO; g.lineWidth = P(7.5);
    g.beginPath(); g.moveTo(P(5), P(-49 + B)); g.lineTo(P(8), P(-56 + B)); g.lineTo(P(9), P(-63 + B)); g.stroke();
    g.fillStyle = "#8f9198"; dot(8, -56 + B, 2.1); g.fill();

    // Head: shell, face plate, camera eye and the flat beak.
    g.translate(P(9), P(-63 + B)); g.rotate(pose.headTilt || 0);
    g.scale(1.28, 1.28); // the head is the biggest part of a MicroDuck
    g.save(); g.translate(P(-2), P(0.6)); g.rotate(pose.beak || 0);
    g.fillStyle = c.beak; box(0, 0, 28, 3.6, 1.8); g.fill(); g.fillStyle = "rgba(0,0,0,.18)"; box(0, 0, 28, 3.6, 1.8); g.fill();
    g.restore();
    g.fillStyle = c.beak; box(-5, -3.6, 33, 5, 2.4); g.fill();
    g.fillStyle = "rgba(255,255,255,.28)"; box(-3, -3.1, 28, 1.4, 0.7); g.fill();
    const shell = () => { g.beginPath(); g.moveTo(P(-13), P(-3.4)); g.lineTo(P(-13), P(-12)); g.quadraticCurveTo(P(-13), P(-23), P(-1), P(-23)); g.lineTo(P(8), P(-23)); g.quadraticCurveTo(P(23), P(-23), P(23), P(-8)); g.lineTo(P(23), P(-3.4)); g.closePath(); };
    g.fillStyle = c.shell; shell(); g.fill();
    g.fillStyle = "rgba(205,206,212,.72)";
    g.beginPath(); g.moveTo(P(-1), P(-3.4)); g.lineTo(P(-1), P(-17.5)); g.quadraticCurveTo(P(3), P(-20), P(9), P(-20)); g.quadraticCurveTo(P(20), P(-20), P(20.4), P(-8)); g.lineTo(P(20.4), P(-3.4)); g.closePath(); g.fill();
    g.fillStyle = "rgba(255,255,255,.3)"; g.beginPath(); g.ellipse(P(-3), P(-19.3), P(8), P(1.9), -0.12, 0, 7); g.fill();
    g.fillStyle = c.eye; dot(9, -12.2, 6.6); g.fill();
    g.fillStyle = "rgba(0,0,0,.14)"; dot(9, -12.2, 6.6); g.fill(); g.fillStyle = c.eye; dot(8.6, -12.7, 5.9); g.fill();
    g.fillStyle = SERVO; dot(9, -12.2, 3.9); g.fill();
    g.fillStyle = "#3a3d52"; dot(9, -12.2, 2.3); g.fill();
    g.fillStyle = "#fff"; dot(7.9, -13.4, 0.95); g.fill();
    g.fillStyle = SERVO; dot(17, -9.3, 1.05); g.fill();
    g.restore();
  }
  // A small canvas with one duck on it, for the cards and the header.
  function portrait(c, pose, px) {
    const cv = document.createElement("canvas"), dpr = window.devicePixelRatio || 1;
    cv.width = cv.height = px * dpr; cv.style.width = cv.style.height = px + "px";
    const g = cv.getContext("2d");
    g.translate(cv.width * 0.38, cv.height * 0.95);
    drawModel(g, c, (cv.height * 0.88) / 92, pose || {});
    return cv;
  }

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


  const facing = [];
  function drawDuck(i, x, y, h, color, state, f, isSel) {
    // Drawn larger than life (a MicroDuck is 25 cm tall) so the ducks read on the map.
    const s = scale, tall = s * 0.5, u = tall / 84, gx = X(x), gy = Y(y);
    const down = state.policy === "fallen" || state.policy === "limp", sit = state.policy === "sit";
    // The sprite faces left or right; it flips only on a clear turn, so it does not flicker.
    if (facing[i] == null || Math.abs(Math.cos(h)) > 0.25) facing[i] = Math.cos(h) >= 0 ? 1 : -1;

    // On the floor: shadow, quack rings, selection ring and the true heading.
    ctx.save(); ctx.translate(gx, gy); ctx.scale(1, 0.42);
    ctx.fillStyle = "rgba(0,0,0,.2)"; ctx.beginPath(); ctx.arc(0, 0, u * (down ? 34 : 22), 0, 7); ctx.fill();
    if (state.quack && !reduced) {
      for (let k = 0; k < 3; k++) {
        const q = (f * 3 + k / 3) % 1;
        ctx.strokeStyle = color; ctx.globalAlpha = (1 - q) * 0.6; ctx.lineWidth = s * 0.014;
        ctx.beginPath(); ctx.arc(0, 0, u * 24 + q * s * 0.55, 0, 7); ctx.stroke();
      }
      ctx.globalAlpha = 1;
    }
    ctx.strokeStyle = color; ctx.lineWidth = s * (isSel ? 0.03 : 0.012); ctx.globalAlpha = isSel ? 1 : 0.7;
    ctx.beginPath(); ctx.arc(0, 0, u * 27, 0, 7); ctx.stroke(); ctx.globalAlpha = 1;
    ctx.restore();
    if (!down) {
      const ax = Math.cos(h), ay = -Math.sin(h) * 0.42, d0 = u * 30, d1 = u * 40, w = u * 7;
      ctx.fillStyle = color; ctx.beginPath();
      ctx.moveTo(gx + ax * d1, gy + ay * d1);
      ctx.lineTo(gx + ax * d0 - ay * w, gy + ay * d0 + ax * w * 0.42);
      ctx.lineTo(gx + ax * d0 + ay * w, gy + ay * d0 - ax * w * 0.42);
      ctx.fill();
    }

    const still = reduced ? 0 : 1, sec = f;
    const pose = {
      down, sit, walking: state.walking && !down && !sit, phase: sec * 9 * still,
      headTilt: sit ? 0.3 : state.state === "look_around" ? Math.sin(sec * 2.4) * 0.24 * still : state.state === "chill" ? Math.sin(sec * 1.1) * 0.05 * still : 0,
      beak: state.quack ? (reduced ? 0.16 : Math.abs(Math.sin(sec * 7)) * 0.2) : 0,
    };
    ctx.save(); ctx.translate(gx, gy); ctx.scale(facing[i], 1);
    drawModel(ctx, state.scheme, u, pose);
    ctx.restore();

    const top = gy - (down ? u * 34 : sit ? u * 76 : u * 94);
    if (sit) { ctx.fillStyle = css("--ink"); ctx.font = `700 ${s * 0.085}px ui-rounded, system-ui`; ctx.fillText("z z", gx + u * 22, top + u * 8); }
    if (down) {
      const r = s * 0.05;
      ctx.fillStyle = css("--bad"); ctx.beginPath(); ctx.arc(gx, top - r, r, 0, 7); ctx.fill();
      ctx.fillStyle = "#fff"; ctx.font = `700 ${r * 1.5}px system-ui`; ctx.textAlign = "center"; ctx.fillText("!", gx, top - r * 0.48); ctx.textAlign = "start";
    }
  }


// ---------- the kernel ----------
function status(text, cls = "") {
  const el = $("status");
  el.textContent = text;
  el.className = "pill " + cls;
}

function send(text, why = "") {
  if (!text) return;
  queue.push({ text, why, urge: why !== "you", at: performance.now() });
  pump();
}

// Which ducks a cue is for, by name, colour or "everyone": the ones that will be thinking.
function ducksNamed(text) {
  const t = text.toLowerCase(), out = new Set();
  const everyone = /\b(everyone|all|ducks)\b/.test(t);
  for (const a of setup.agents) if (everyone || t.includes(a.agent.toLowerCase()) || (a.colour && t.includes(a.colour))) out.add(a.agent);
  return out;
}

function pump() {
  if (cuePending || !queue.length || !bodyReady || !brainReady) return;
  // An urge that waited too long behind a busy brain is stale: the duck has moved on.
  while (queue.length && queue[0].urge && performance.now() - queue[0].at > 12000) queue.shift();
  if (!queue.length) return;
  const next = queue.shift();
  cuePending = true;
  thinking = ducksNamed(next.text);
  brain.postMessage({ type: "think", id: ++cueId, text: next.text, briefs, agents: setup.agents.map((a) => a.agent) });
  next.why && logLine(`${next.text}`, next.why);
  updateCards();
}

function logLine(head, body) {
  const log = $("log");
  log.querySelector(".empty")?.remove();
  const li = document.createElement("li");
  const b = document.createElement("strong"); b.textContent = head;
  const s = document.createElement("small"); s.textContent = body;
  li.append(b, s);
  log.prepend(li);
  while (log.children.length > 24) log.lastElementChild.remove();
}

function describeCue(result) {
  for (const turn of result.turns) {
    for (const attempt of turn.attempts) {
      if (!attempt.calls.length) {
        lastBrain[turn.agent] = { text: `ignored "${attempt.clause}"`, verdict: attempt.ignored || "" };
        continue;
      }
      for (const c of attempt.calls) {
        const args = Object.values(c.call.arguments ?? {}).join(", ");
        const verdict = c.verdicts.map((v) => v.verdict).join(", ") || "allow";
        const text = c.error ? `${c.call.name}(${args}): ${c.error}` : `${c.call.name}(${args})`;
        lastBrain[turn.agent] = { text, verdict, confidence: attempt.confidence };
        logLine(`${turn.agent}: ${text}`, `"${attempt.clause}" · safety ${verdict}`);
      }
    }
    if (turn.error) lastBrain[turn.agent] = { text: "brain error", verdict: String(turn.error).slice(0, 80) };
  }
}

body.onmessage = ({ data: m }) => {
  if (m.type === "ready") {
    setup = m.setup;
    briefs = m.briefs;
    data.room = setup.room;
    bodyReady = true;
    $("main").hidden = false;
    // The pond is alive before the model has finished loading.
    if (!brainReady) status("the brains are waking up");
    suggestions();
    resize();
    loop();
  } else if (m.type === "frame") {
    renderInFlight = false;
    onFrame(m.frame);
  } else if (m.type === "delivered") {
    cuePending = false;
    thinking = new Set();
    describeCue({ turns: m.turns });
    for (const t of m.turns) if (!lastBrain[t.agent]) lastBrain[t.agent] = { text: "no answer", verdict: "" };
    updateCards();
    pump();
  } else if (m.type === "error") {
    status("error: " + m.message, "bad");
    cuePending = false; renderInFlight = false; thinking = new Set();
  }
};

brain.onmessage = ({ data: m }) => {
  if (m.type === "progress") {
    if (brainReady) return;
    const pct = m.total ? ` ${Math.round((m.loaded / m.total) * 100)}%` : "";
    status(`loading the brains: ${m.label}${pct}`);
  } else if (m.type === "ready") {
    brainReady = true;
    status("live", "live");
    pump();
  } else if (m.type === "thought") {
    // What the brains decided goes to the bodies, which check and apply it.
    body.postMessage({ type: "deliver", id: m.id, turns: m.turns });
  } else if (m.type === "error") {
    status("error: " + m.message, "bad");
    cuePending = false; thinking = new Set();
  }
};

body.postMessage({ type: "init" });
brain.postMessage({ type: "init" });

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

// The bodies tick ten times a second on a timer, whatever the brains are doing and
// whatever the display's refresh is. The page draws every display frame in between,
// easing from the last body frame to the new one. Animation clocks (legs, beaks,
// rings) use the wall clock, so they run even when no frame arrives.
const TICK_MS = 100;
let renderInFlight = false, frameTick = 0, lastDraw = 0;
let prevFrame = null, curFrame = null, curAt = 0, interval = TICK_MS;
function loop() {
  // A browser throttles a hidden tab's timers and animation frames by itself, so the pond rests there.
  setInterval(() => {
    if (renderInFlight) return;
    renderInFlight = true;
    body.postMessage({ type: "render", tick: frameTick++ });
  }, TICK_MS);
  const paint = (now) => {
    if (lastDraw) window.__pond.maxGapMs = Math.max(window.__pond.maxGapMs, now - lastDraw);
    lastDraw = now; window.__pond.draws++;
    draw(now);
    requestAnimationFrame(paint);
  };
  requestAnimationFrame(paint);
}

function onFrame(f) {
  const now = performance.now();
  if (curFrame) interval = Math.min(400, Math.max(50, now - curAt));
  prevFrame = curFrame; curFrame = f; curAt = now; frame = f;
  window.__pond.frames++;
  // Idle ducks ask their brains for something to do; the brains answer through the safety control.
  for (const u of f.urges || []) queueUrge(u);
  for (const e of f.events || []) feedEvent(e);
  updateCards();
  // Also draw here: where a browser starves animation frames, the pond still moves at the body's own rate.
  draw(now);
}

const mix = (a, b, k) => a + (b - a) * k;
const mixAngle = (a, b, k) => a + ((((b - a + Math.PI) % (2 * Math.PI)) + 2 * Math.PI) % (2 * Math.PI) - Math.PI) * k;
// Where duck `i` is right now, between the last two frames.
function placed(i, now) {
  const b = curFrame.ducks[i];
  if (!prevFrame) return { x: b.x, y: b.y, heading: b.heading };
  const a = prevFrame.ducks[i], k = Math.min(1, (now - curAt) / interval);
  return { x: mix(a.x, b.x, k), y: mix(a.y, b.y, k), heading: mixAngle(a.heading, b.heading, k) };
}

const urgeAsked = new Set();
function queueUrge(u) {
  // Before the brains are loaded nobody can answer; the duck will ask again later.
  if (!brainReady) return;
  const name = setup.agents[u.agent].agent;
  // One pending urge per duck: a duck that is still waiting does not ask again.
  if (urgeAsked.has(name)) return;
  urgeAsked.add(name);
  setTimeout(() => urgeAsked.delete(name), 6000);
  send(u.text, u.why);
}

const recentEvents = [];
function feedEvent(e) {
  recentEvents.push(e);
  if (recentEvents.length > 8) recentEvents.shift();
  if (e.text.startsWith("quacks")) logLine(`${e.agent} quacks`, e.text.replace(/^quacks: /, "because "));
}

function draw(now = performance.now()) {
  if (!curFrame) return;
  const f = now / 1000;
  drawRoom();
  const ball = prevFrame ? [mix(prevFrame.ball[0], curFrame.ball[0], Math.min(1, (now - curAt) / interval)), mix(prevFrame.ball[1], curFrame.ball[1], Math.min(1, (now - curAt) / interval))] : curFrame.ball;
  ctx.save(); ctx.shadowColor = "rgba(0,0,0,.3)"; ctx.shadowBlur = scale * 0.03;
  ctx.fillStyle = "#f6883a"; ctx.beginPath(); ctx.arc(X(ball[0]), Y(ball[1]), scale * 0.05, 0, 7); ctx.fill(); ctx.restore();
  const sprites = curFrame.ducks.map((d, i) => ({ i, d, p: placed(i, now), color: COLORS[i % COLORS.length] }));
  sprites.sort((a, b) => b.p.y - a.p.y);
  for (const sp of sprites) {
    const d = sp.d;
    headings[sp.i] = sp.p.heading;
    drawDuck(sp.i, sp.p.x, sp.p.y, sp.p.heading, sp.color, {
      policy: d.policy, state: d.action, quack: d.action === "quack", scheme: SCHEMES[sp.i], walking: d.policy === "walk",
    }, f, sp.i === selected);
  }
  ctx.font = `600 ${scale * 0.062}px ui-rounded, system-ui`; ctx.textAlign = "center"; ctx.textBaseline = "middle";
  for (const sp of sprites) {
    const w = ctx.measureText(sp.d.agent).width + scale * 0.07, hgt = scale * 0.095, cx = X(sp.p.x), cy = Y(sp.p.y) + scale * 0.13;
    ctx.fillStyle = css("--card"); ctx.globalAlpha = 0.88; rr(cx - w / 2, cy - hgt / 2, w, hgt, hgt / 2); ctx.fill(); ctx.globalAlpha = 1;
    ctx.strokeStyle = sp.color; ctx.lineWidth = scale * 0.008; ctx.stroke();
    ctx.fillStyle = css("--ink"); ctx.fillText(sp.d.agent, cx, cy + scale * 0.004);
  }
  // A duck whose brain is working shows it: three dots in a bubble over its head.
  for (const sp of sprites) {
    if (!thinking.has(sp.d.agent)) continue;
    const dots = reduced ? 3 : 1 + (Math.floor(now / 300) % 3), label = ".".repeat(dots).padEnd(3, "\u00a0");
    const bx = X(sp.p.x) + scale * 0.14, by = Y(sp.p.y) - scale * 0.5, w = scale * 0.2, hh = scale * 0.1;
    ctx.fillStyle = css("--card"); ctx.strokeStyle = css("--accent"); ctx.lineWidth = scale * 0.008;
    rr(bx - w / 2, by - hh / 2, w, hh, hh / 2); ctx.fill(); ctx.stroke();
    ctx.fillStyle = css("--ink"); ctx.font = `700 ${scale * 0.09}px ui-rounded, system-ui`;
    ctx.fillText(label, bx, by - scale * 0.01);
  }
  ctx.textAlign = "start"; ctx.textBaseline = "alphabetic";
}

let cardsSig = "";
function updateCards() {
  if (!frame) return;
  // Frames come ten times a second; the cards only need rebuilding when something on them changed.
  const sig = JSON.stringify([frame.ducks.map((d) => [d.action, d.place, d.policy, Math.round(d.energy * 100)]), lastBrain, selected, [...thinking]]);
  if (sig === cardsSig) return;
  cardsSig = sig;
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
    const thought = document.createElement("div"); thought.className = "thought";
    const b = lastBrain[d.agent];
    thought.textContent = thinking.has(d.agent) ? "thinking…" : b ? `${b.text}${b.verdict ? "  ·  " + b.verdict : ""}` : "";
    const energy = document.createElement("div"); energy.className = "energy";
    const bar = document.createElement("div"); bar.className = "bar";
    const fill = document.createElement("i"); fill.style.width = Math.round(d.energy * 100) + "%";
    fill.className = d.energy < 0.2 ? "low" : d.energy < 0.5 ? "mid" : "";
    bar.append(fill); energy.append("energy", bar, Math.round(d.energy * 100) + "%");
    card.append(head, thought, energy);
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
