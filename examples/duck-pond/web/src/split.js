// The two halves of a cue, kept apart so the bodies never wait for a brain.
// A cue used to run in one go on one thread (`kernel.cue`), and the model's time
// froze the bodies with it. Now the brain half runs in its own worker, and the page
// hands what it decided to the body half.

/**
 * Brain half: ask every duck's actor for tool calls. This takes the model's time
 * (about 0.8 s per duck that is addressed) and blocks the thread it runs on, which
 * is why it must not be the thread that moves the bodies.
 * @param actor   an `actor` interface instance
 * @param briefs  each duck's brief, as the stage wrote it (a JSON string)
 * @param agents  [{ agent }] in the same order
 */
export function think(actor, briefs, agents, text) {
  return agents.map((agent, i) => {
    const started = performance.now();
    const turn = { agent: agent.agent ?? agent, attempts: [], error: null };
    try {
      turn.attempts = JSON.parse(actor.act(briefs[i], text));
    } catch (e) {
      turn.error = String(e.payload?.val ?? e.message ?? e);
    }
    turn.latency_ms = Math.round(performance.now() - started);
    return turn;
  });
}

/**
 * Body half: pass every call a brain proposed through the controls and apply it.
 * Fast, so the bodies' thread barely notices. Returns the turns in the shape
 * `kernel.cue` gives, with a record for each call.
 */
export function deliver(kernel, turns) {
  return turns.map((turn, i) => ({
    ...turn,
    attempts: turn.attempts.map((attempt) => ({
      ...attempt,
      calls: attempt.calls.map((call) => {
        try {
          return kernel.call(i, call, attempt.clause).turns[0].attempts[0].calls[0];
        } catch (e) {
          return { call, delivered: null, verdicts: [], result: null, error: String(e.payload ?? e.message ?? e) };
        }
      }),
    })),
  }));
}
