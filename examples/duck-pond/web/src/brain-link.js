// The page's side of the kernel worker, shared by the duck pages: it sends cues
// one at a time, keeps the body ticking, and writes what the brains decided to
// the log. The page supplies what to do when the kernel is ready and on each frame.

/**
 * @param worker    the page's kernel worker
 * @param statusEl  element that shows loading progress and the last brain time
 * @param logEl     list element for the log of decisions
 * @param onReady   (setup, controls) => void, once the kernel is up
 * @param onFrame   (frame) => void, after every body tick
 * @param onCue     () => void, after the brains answered a cue
 */
export function connectBrains({ worker, statusEl, logEl, onReady, onFrame, onCue = () => {} }) {
  const queue = [];        // cues waiting for the brain: { text, why }
  const brainMs = [];      // how long each cue took in the worker
  const lastBrain = {};    // duck name -> the last thing its brain did
  let cuePending = false, cueId = 0, ready = false, renderPending = false;

  function status(text, cls = "") {
    statusEl.textContent = text;
    statusEl.className = "pill " + cls;
  }

  function logLine(head, body, cls = "") {
    logEl.querySelector(".empty")?.remove();
    const li = document.createElement("li");
    if (cls) li.className = cls;
    const b = document.createElement("strong"); b.textContent = head;
    const s = document.createElement("small"); s.textContent = body;
    li.append(b, s);
    logEl.prepend(li);
    while (logEl.children.length > 24) logEl.lastElementChild.remove();
  }

  function pump() {
    if (cuePending || !queue.length || !ready) return;
    const next = queue.shift();
    cuePending = true;
    worker.postMessage({ type: "cue", id: ++cueId, text: next.text });
    if (next.why) logLine(next.text, next.why, "asked");
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
          const ran = c.delivered && c.delivered.name !== c.call.name ? ` → ${c.delivered.name}` : "";
          const text = c.error ? `${c.call.name}(${args}): ${c.error}` : `${c.call.name}(${args})${ran}`;
          const why = c.verdicts.find((v) => v.verdict === "block")?.detail || "";
          lastBrain[turn.agent] = { text, verdict, confidence: attempt.confidence };
          logLine(`${turn.agent}: ${text}`, `"${attempt.clause}" · ${verdict}${why ? ": " + why : ""}`, verdict.split(",")[0]);
        }
      }
      if (turn.error) lastBrain[turn.agent] = { text: "brain error", verdict: String(turn.error).slice(0, 80) };
    }
  }

  worker.onmessage = ({ data: m }) => {
    if (m.type === "progress") {
      const pct = m.total ? ` ${Math.round((m.loaded / m.total) * 100)}%` : "";
      status(`loading ${m.label}${pct}`);
    } else if (m.type === "ready") {
      ready = true;
      status("live", "live");
      onReady(m.setup, m.controls);
    } else if (m.type === "frame") {
      renderPending = false;
      onFrame(m.frame);
    } else if (m.type === "cue-result") {
      cuePending = false;
      describeCue(m.result);
      if (m.ms != null) { brainMs.push(m.ms); status(`live · brain ${m.ms} ms`, "live"); }
      onCue();
      pump();
    } else if (m.type === "error") {
      status("error: " + m.message, "bad");
      cuePending = false;
      renderPending = false;
    }
  };
  worker.postMessage({ type: "init" });

  return {
    brainMs,
    lastBrain,
    logLine,
    /** Queues one line for the brains. `why` is shown with it in the log. */
    send(text, why = "") {
      if (!text) return;
      queue.push({ text, why });
      pump();
    },
    /** Asks for the next body tick(s), unless one is still on its way. */
    render(tick, steps = 1) {
      if (!ready || renderPending) return false;
      renderPending = true;
      worker.postMessage({ type: "render", tick, steps });
      return true;
    },
  };
}
