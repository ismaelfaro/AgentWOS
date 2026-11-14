// Spikes: short streaks of light that travel from one agent card to another
// when a signal passes between them (the bar clock, a new root, a cue).

const SVG = "http://www.w3.org/2000/svg";
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)");

/**
 * @param container  positioned element that holds the cards; the overlay
 *                   covers it and never takes pointer events
 */
export function createSignals(container) {
  const overlay = document.createElementNS(SVG, "svg");
  overlay.setAttribute("class", "signals");
  overlay.setAttribute("aria-hidden", "true");

  /** Centre of an element, in the overlay's coordinates. */
  function centre(el) {
    const box = container.getBoundingClientRect(), r = el.getBoundingClientRect();
    return [r.left - box.left + r.width / 2, r.top - box.top + r.height / 2];
  }

  /**
   * Sends one spike from `from` to `to` (elements inside or outside the
   * container). `strength` 0..1 sets how bright and thick it is.
   */
  function spike(from, to, { color = "var(--accent)", strength = 1, delay = 0 } = {}) {
    if (!from || !to || from === to || reducedMotion.matches || document.hidden) return;
    const [x1, y1] = centre(from), [x2, y2] = centre(to);
    const dx = x2 - x1, dy = y2 - y1, dist = Math.hypot(dx, dy);
    if (dist < 8) return;
    // Bow the line sideways so spikes between neighbours do not hide behind the cards' edges.
    const bend = Math.min(60, dist * 0.18);
    const cx = (x1 + x2) / 2 - (dy / dist) * bend, cy = (y1 + y2) / 2 + (dx / dist) * bend;

    const path = document.createElementNS(SVG, "path");
    path.setAttribute("d", `M ${x1} ${y1} Q ${cx} ${cy} ${x2} ${y2}`);
    path.setAttribute("fill", "none");
    path.style.stroke = color;
    path.setAttribute("stroke-linecap", "round");
    path.setAttribute("stroke-width", String(1.5 + 3 * strength));
    path.style.opacity = String(0.25 + 0.75 * strength);
    overlay.append(path);

    const length = path.getTotalLength();
    const streak = Math.min(length * 0.35, 30 + 50 * strength);
    path.setAttribute("stroke-dasharray", `${streak} ${length + streak}`);
    const travel = path.animate(
      [{ strokeDashoffset: streak }, { strokeDashoffset: -length }],
      { duration: 260 + dist * 0.9, delay, easing: "ease-in", fill: "backwards" },
    );
    travel.onfinish = () => {
      path.remove();
      if (strength >= 0.6) {
        to.animate(
          [{ boxShadow: `0 0 0 3px ${color}` }, { boxShadow: "0 0 0 0 transparent" }],
          { duration: 500, easing: "ease-out" },
        );
      }
    };
  }

  return {
    /** Call after the container's children are replaced: keeps the overlay on top. */
    attach() {
      container.append(overlay);
    },
    spike,
  };
}
