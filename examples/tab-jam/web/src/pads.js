// XY touch pads. Each pad maps two settings of one agent to a surface: the
// crosshair shows the values the band is actually using, and a ring shows where
// the finger asked for. They differ when an OS control changed or blocked the call.

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const pretty = (v) => String(v).replaceAll("_", " ");

/** Position of `value` along an axis, 0..1, or null when the axis cannot show it. */
function fraction(axis, value) {
  if (axis.values) {
    const i = axis.values.indexOf(value);
    return i < 0 ? null : (i + 0.5) / axis.values.length;
  }
  return typeof value === "number" ? clamp((value - axis.min) / (axis.max - axis.min), 0, 1) : null;
}

/** Value at a position 0..1 along an axis, snapped to its options or step. */
function valueAt(axis, f) {
  if (axis.values) return axis.values[clamp(Math.floor(f * axis.values.length), 0, axis.values.length - 1)];
  const raw = axis.min + f * (axis.max - axis.min);
  return clamp(Math.round(raw / axis.step) * axis.step, axis.min, axis.max);
}

function stepped(axis, value, delta) {
  if (axis.values) {
    const i = Math.max(0, axis.values.indexOf(value));
    return axis.values[clamp(i + delta, 0, axis.values.length - 1)];
  }
  return clamp((typeof value === "number" ? value : axis.min) + delta * axis.step, axis.min, axis.max);
}

function ticks(axis) {
  if (axis.values) return axis.values.map(pretty);
  const mid = Math.round((axis.min + axis.max) / 2 / axis.step) * axis.step;
  return [axis.min, mid, axis.max].map(String);
}

function node(tag, className, text) {
  const n = document.createElement(tag);
  if (className) n.className = className;
  if (text !== undefined) n.textContent = text;
  return n;
}

/**
 * @param desc     { x, y } axes from the stage: label, tool, arg, state, and
 *                 either `values` or `min`/`max`/`step`
 * @param name     agent name, for the accessible label
 * @param onChange called with { x?, y? } holding the values the user asked for
 */
export function createPad(desc, name, onChange) {
  const { x, y } = desc;
  const root = node("div", "pad");
  const yTicks = node("div", "pad-y" + (y.values ? " cells" : ""));
  for (const t of ticks(y).reverse()) yTicks.append(node("span", "", t));
  const area = node("div", "pad-area");
  area.tabIndex = 0;
  area.setAttribute("role", "group");
  area.setAttribute("aria-label", `${name} pad: ${x.label} across, ${y.label} up. Drag, or use the arrow keys.`);
  const xTicks = node("div", "pad-x" + (x.values ? " cells" : ""));
  for (const t of ticks(x)) xTicks.append(node("span", "", t));

  // Faint grid: one cell per option on a stepped axis.
  const cols = x.values ? x.values.length : 4;
  const rows = y.values ? y.values.length : 4;
  area.style.backgroundSize = `${100 / cols}% ${100 / rows}%`;

  const lineV = node("div", "cross cross-v");
  const lineH = node("div", "cross cross-h");
  const dot = node("div", "pad-dot");
  const ghost = node("div", "pad-ghost");
  const readout = node("div", "pad-readout");
  area.append(lineV, lineH, dot, ghost);
  root.append(readout, yTicks, area, node("div", "pad-corner", `${y.label} / ${x.label}`), xTicks);

  let current = { x: null, y: null };
  let asked = { x: null, y: null };
  let ghostTimer = null;

  function place() {
    const fx = fraction(x, current.x), fy = fraction(y, current.y);
    lineV.hidden = fx === null;
    lineH.hidden = fy === null;
    dot.hidden = fx === null || fy === null;
    if (fx !== null) lineV.style.left = dot.style.left = fx * 100 + "%";
    if (fy !== null) lineH.style.top = dot.style.top = (1 - fy) * 100 + "%";
    readout.textContent = `${x.label} ${pretty(current.x)} - ${y.label} ${pretty(current.y)}`;
  }

  function request(next) {
    const change = {};
    if (next.x !== asked.x) change.x = next.x;
    if (next.y !== asked.y) change.y = next.y;
    asked = next;
    const gx = fraction(x, next.x), gy = fraction(y, next.y);
    ghost.style.left = gx * 100 + "%";
    ghost.style.top = (1 - gy) * 100 + "%";
    ghost.classList.add("on");
    clearTimeout(ghostTimer);
    ghostTimer = setTimeout(() => ghost.classList.remove("on"), 1200);
    if ("x" in change || "y" in change) onChange(change);
  }

  function fromPointer(e) {
    const r = area.getBoundingClientRect();
    request({
      x: valueAt(x, clamp((e.clientX - r.left) / r.width, 0, 0.9999)),
      y: valueAt(y, clamp(1 - (e.clientY - r.top) / r.height, 0, 0.9999)),
    });
  }

  let dragging = false;
  area.addEventListener("pointerdown", (e) => {
    dragging = true;
    // Capture keeps the drag alive when the finger leaves the pad; it is not
    // available for every pointer, and the pad works without it.
    try { area.setPointerCapture(e.pointerId); } catch {}
    asked = { ...current };
    fromPointer(e);
  });
  area.addEventListener("pointermove", (e) => {
    if (dragging) fromPointer(e);
  });
  for (const type of ["pointerup", "pointercancel"]) {
    area.addEventListener(type, () => { dragging = false; });
  }
  area.addEventListener("keydown", (e) => {
    const move = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowDown: [0, -1], ArrowUp: [0, 1] }[e.key];
    if (!move) return;
    e.preventDefault();
    asked = { ...current };
    request({ x: stepped(x, current.x, move[0]), y: stepped(y, current.y, move[1]) });
  });

  return {
    element: root,
    /** Moves the crosshair to the values in the band state. */
    show(state) {
      current = { x: state[x.state], y: state[y.state] };
      place();
    },
  };
}
