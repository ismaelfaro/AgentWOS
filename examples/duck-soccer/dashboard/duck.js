// The MicroDuck drawing, shared by the replay and the lab pages.
(() => {
  const SCHEMES = [
    { shell: "#8fd6e3", beak: "#f2682a", eye: "#f5a623", foot: "#f2682a", sole: "#f5b73a", accent: "#3fb6cc" },
    { shell: "#6c6e75", beak: "#f4cd3c", eye: "#8b6fd0", foot: "#f4cd3c", sole: "#8b6fd0", accent: "#e0b21f" },
    { shell: "#ece5d6", beak: "#f2682a", eye: "#f59a1f", foot: "#f59a1f", sole: "#f2682a", accent: "#f2682a" },
    { shell: "#bda8dc", beak: "#f4dc4a", eye: "#7fd0de", foot: "#f4dc4a", sole: "#8b6fd0", accent: "#9a7fd0" },
  ];
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
  window.Duck = { SCHEMES, drawModel };
})();
