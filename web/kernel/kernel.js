// AgentWOS browser kernel. Same role as the Rust host: instantiate components,
// mediate every call between them, run the control chain and record what happened.
// No dependencies: the embedding page passes in the WASI shim and the components.

/**
 * Builds the import object a jco-transpiled component expects. Keys look like
 * "wasi:cli/environment@0.2.12"; a Proxy resolves them against the shim so the
 * kernel does not hardcode WASI versions.
 */
export function wasiImports(shim, extra = {}) {
  const camel = (s) => s.replace(/-([a-z])/g, (_, c) => c.toUpperCase());
  const unversioned = (key) => key.replace(/@.*$/, "");
  const own = Object.fromEntries(Object.entries(extra).map(([k, v]) => [unversioned(k), v]));
  return new Proxy({}, {
    get(_, key) {
      if (typeof key !== "string") return undefined;
      const name = unversioned(key);
      if (name in own) return own[name];
      const m = /^wasi:([a-z-]+)\/([a-z-]+)$/.exec(name);
      return m ? shim[m[1]]?.[camel(m[2])] : undefined;
    },
  });
}

/** Fetches and compiles the core wasm files of one transpiled component. */
export function coreModuleLoader(baseUrl) {
  return (path) => WebAssembly.compileStreaming(fetch(new URL(path, baseUrl)));
}

/**
 * @param stage     instance of the `stage` interface
 * @param actors    one `actor` interface instance per agent, in stage order
 * @param controls  [{ name, control }] applied in order to every tool call
 */
export function createKernel({ stage, actors, controls }) {
  const setup = JSON.parse(stage.setup("{}"));
  const agents = setup.agents;

  /** Runs the control chain on one tool call. Returns the delivered call or null. */
  function inspect(author, call) {
    let current = JSON.stringify(call);
    const verdicts = [];
    for (const { name, control } of controls) {
      let verdict;
      try {
        verdict = control.inspect(author, current, stage.context());
      } catch (e) {
        // A failing control must not stop the session: log it and carry on.
        verdicts.push({ module: name, verdict: "error", detail: String(e) });
        continue;
      }
      if (verdict.tag === "modify") {
        verdicts.push({ module: name, verdict: "modify", detail: `${current} -> ${verdict.val}` });
        current = verdict.val;
      } else {
        verdicts.push({ module: name, verdict: verdict.tag, detail: verdict.val ?? "" });
        if (verdict.tag === "block") return { delivered: null, verdicts };
      }
    }
    return { delivered: JSON.parse(current), verdicts };
  }

  /** Sends one instruction to every agent and applies what the controls let through. */
  function cue(text) {
    const turns = agents.map((agent, i) => {
      const started = performance.now();
      const turn = { agent: agent.agent, attempts: [], error: null };
      try {
        const attempts = JSON.parse(actors[i].act(stage.brief(i), text));
        turn.attempts = attempts.map((attempt) => ({
          ...attempt,
          calls: attempt.calls.map((call) => {
            const { delivered, verdicts } = inspect(agent.agent, call);
            const record = { call, delivered, verdicts, result: null, error: null };
            if (delivered) {
              try {
                record.result = stage.apply(i, JSON.stringify(delivered));
              } catch (e) {
                record.error = String(e.payload ?? e.message ?? e);
              }
            }
            return record;
          }),
        }));
      } catch (e) {
        turn.error = String(e.payload?.val ?? e.message ?? e);
      }
      turn.latency_ms = Math.round(performance.now() - started);
      return turn;
    });
    return { text, turns, state: JSON.parse(stage.state()) };
  }

  return {
    setup,
    agents,
    cue,
    state: () => JSON.parse(stage.state()),
    render: (tick) => JSON.parse(stage.render(tick)),
    briefs: () => agents.map((_, i) => JSON.parse(stage.brief(i))),
  };
}
