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

/**
 * Fetches and compiles the core wasm files of one transpiled component.
 * Serve each build from its own folder: JS glue paired with core modules cached
 * from an older build fails at instantiation.
 */
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

  /**
   * Applies one tool call for an agent without asking the model, for direct
   * manipulation in a UI. It still goes through the control chain, so a control
   * can modify or block it exactly as it would a call the model proposed.
   */
  function call(agentIndex, toolCall, source = "direct") {
    const agent = agents[agentIndex];
    const started = performance.now();
    const { delivered, verdicts } = inspect(agent.agent, toolCall);
    const record = { call: toolCall, delivered, verdicts, result: null, error: null };
    if (delivered) {
      try {
        record.result = stage.apply(agentIndex, JSON.stringify(delivered));
      } catch (e) {
        record.error = String(e.payload ?? e.message ?? e);
      }
    }
    const turn = {
      agent: agent.agent,
      attempts: [{ clause: source, confidence: null, calls: [record], ignored: null }],
      error: null,
      latency_ms: Math.round(performance.now() - started),
    };
    return { text: `${source}: ${agent.agent}`, source, turns: [turn], state: JSON.parse(stage.state()) };
  }

  /**
   * Adds an agent while running. The request goes through the control chain
   * like any other call, so a control can refuse it. `makeActor` creates the
   * new agent's actor instance; it is only called once the controls agree.
   */
  async function spawn(kind, makeActor) {
    const started = performance.now();
    const request = { name: "spawn_agent", arguments: { kind } };
    const { delivered, verdicts } = inspect("kernel", request);
    const record = { call: request, delivered, verdicts, result: null, error: null };
    let added = null;
    if (delivered) {
      try {
        const actor = await makeActor();
        const description = JSON.parse(stage.spawn(delivered.arguments.kind));
        agents.push({ agent: description.agent, role: description.role });
        actors.push(actor);
        added = { ...description, brief: JSON.parse(stage.brief(description.index)) };
        record.result = `${description.agent} joined`;
      } catch (e) {
        record.error = String(e.payload ?? e.message ?? e);
      }
    }
    const turn = {
      agent: "Kernel",
      attempts: [{ clause: "spawn", confidence: null, calls: [record], ignored: null }],
      error: null,
      latency_ms: Math.round(performance.now() - started),
    };
    return { text: `add agent: ${kind}`, source: "spawn", turns: [turn], state: JSON.parse(stage.state()), added };
  }

  return {
    setup,
    agents,
    cue,
    call,
    spawn,
    state: () => JSON.parse(stage.state()),
    render: (tick) => JSON.parse(stage.render(tick)),
    briefs: () => agents.map((_, i) => JSON.parse(stage.brief(i))),
  };
}
