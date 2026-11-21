// How a MicroDuck is drawn: the four colourways, the side-view model, a small
// portrait canvas, and a duck standing on a floor plan. Shared by the duck
// examples' pages; it knows nothing about a room or a game.

export const SCHEMES = [
  { shell: "#8fd6e3", beak: "#f2682a", eye: "#f5a623", foot: "#f2682a", sole: "#f5b73a", accent: "#3fb6cc" },
  { shell: "#6c6e75", beak: "#f4cd3c", eye: "#8b6fd0", foot: "#f4cd3c", sole: "#8b6fd0", accent: "#e0b21f" },
  { shell: "#ece5d6", beak: "#f2682a", eye: "#f59a1f", foot: "#f59a1f", sole: "#f2682a", accent: "#f2682a" },
  { shell: "#bda8dc", beak: "#f4dc4a", eye: "#7fd0de", foot: "#f4dc4a", sole: "#8b6fd0", accent: "#9a7fd0" },
];
export const COLORS = SCHEMES.map((c) => c.accent);
export const SERVO = "#1b1b20";
// One model of a MicroDuck, side view facing right, drawn on any canvas: the
// map, the cards and the header use it. Origin is the ground under the body,
// `u` is one model unit in pixels (the duck is 84 units tall).
export function drawModel(g, c, u, pose) {
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
export function portrait(c, pose, px) {
  const cv = document.createElement("canvas"), dpr = window.devicePixelRatio || 1;
  cv.width = cv.height = px * dpr; cv.style.width = cv.style.height = px + "px";
  const g = cv.getContext("2d");
  g.translate(cv.width * 0.38, cv.height * 0.95);
  drawModel(g, c, (cv.height * 0.88) / 92, pose || {});
  return cv;
}

const facing = [];
/**
 * Draws one duck standing on the floor at (x, y), facing `h`.
 * `view` is the canvas it draws on: { ctx, scale, X, Y, css, reduced, tall? }.
 * `tall` is how high a duck is drawn, in metres of the floor plan (default 0.5).
 * `state`: { scheme, policy, state, quack, walking, team? }. `f` is the time in seconds.
 */
export function drawDuck(view, i, x, y, h, color, state, f, isSel) {
  const { ctx, scale, X, Y, css, reduced, tall: height = 0.5 } = view;
  // Drawn larger than life (a MicroDuck is 25 cm tall) so the ducks read on the map.
  const s = scale, tall = s * height, u = tall / 84, gx = X(x), gy = Y(y);
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
