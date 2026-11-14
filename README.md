# AgentWOS: an agent operating system on WebAssembly

**AgentWOS** is an experimental operating system for AI agents, built on
WebAssembly and WASI. Every agent, tool, LLM client, OS control and scenario is
a WASM component with its own sandbox. A Rust host (Wasmtime and the Component
Model) runs them, mediates every call between them, checks every action and
records what happened. The same components also run in a browser tab, with no
server.

Early-stage and open. Built to learn how agents should be isolated, governed and
composed, and to show it working.

**Live example:** [**Agentic Tab Jam**](https://agentic-tab-jam.asst.workers.dev),
a band of WASM agents you conduct in plain English, running fully in your
browser with [Needle](https://cactuscompute.com/needle), a 35 MB tool-calling
model. It is one example of what AgentWOS runs, not the whole project.

## Why an operating system for agents

Most agent frameworks put every agent in one process, trust every tool, and
leave policy in prompts. AgentWOS takes the operating-system view instead:

- **Isolation by default.** One WASI context per instance. A tool or agent gets
  no network, files or secrets unless its manifest grants them.
- **Mediated calls.** Agents never call each other directly. Every tool call,
  model call and message goes through the host, where it can be inspected.
- **Controls as processes.** An OS control is a WASM component that sees every
  action and can allow, modify, block or flag it. It describes its own rules,
  so a UI can show what it enforces.
- **Portable components.** The same `.wasm` runs in the native host and in the
  browser. A contract in one WIT file (`wit/world.wit`) generates the bindings
  for both.
- **Observable runs.** Every run writes a log that a dashboard reads, with the
  verdict and latency of each action.

## Two ways to run it

| | Native host (`host/`) | Browser kernel (`web/kernel/`) |
|---|---|---|
| Runtime | Wasmtime, Component Model | The browser's WebAssembly, components transpiled with `jco` |
| Model | Any OpenAI-compatible API: LM Studio, OpenRouter | Needle (35 MB), in the tab |
| Example | [Mars colony](examples/mars-colony/) (turn-based scenario) | [Tab Jam](examples/tab-jam/) (interactive, tool calls) |
| Also | [CLI harness](examples/cli-harness/): a coding harness of small expert agents | |

## Quick start

Native host, with LM Studio on `localhost:1234` or an OpenRouter key:

```bash
source "$HOME/.cargo/env"
for m in modules/tool-echo modules/llm-call modules/agent-core; do
  (cd $m && cargo build --release --target wasm32-wasip2)
done
cargo build --release

examples/mars-colony/build.sh
./target/release/agentwos-host --manifest examples/mars-colony/manifest.json --colony 3
```

Browser, no server needed after the first load:

```bash
examples/tab-jam/build.sh
tools/serve.py
# open http://localhost:8765/examples/tab-jam/web/dist/index.html
```

Requirements: Rust with the `wasm32-wasip2` target (`rustup target add wasm32-wasip2`),
and Python 3 to serve the pages.

## Examples

- **[Agentic Tab Jam](examples/tab-jam/)**: a browser band. Agents
  (drums, bass, lead, and orchestra sections you add one by one) act only
  through tool calls that Needle proposes. An Orchestra Director control checks
  each call, and spikes show the signals between agents.
- **[Mars colony](examples/mars-colony/)**: a turn-based simulation. Five
  colonists decide each day what to do, a control filters their actions, and a
  dashboard replays the run.
- **[CLI harness](examples/cli-harness/)**: a coding harness where every plugin is a
  small agent. A lead agent edits the project and asks experts (git, Python,
  Rust, JavaScript, search, review) for the rest; which experts take part
  depends on the project. Each has only the grants its manifest gives it: an
  LLM, the project folder, its own programs, the other plugins.

## Architecture

```
  host (Rust, Wasmtime) = the kernel
  ┌──────────────────────────────────────────────────────────────────┐
  │ runner      drives a scenario: turns, controls, log              │
  │ runtime     one Store per instance, WASI, component loading      │
  │ control     chain of OS control modules, applied to every action │
  └──────────────────────────────────────────────────────────────────┘
        │                         │                        │
        ▼                         ▼                        ▼
  scenario.wasm            agent-core.wasm           control-*.wasm
  (world, prompts,         ──import tool──▶ host      inspect(author, text,
   rules, record)          ──import llm───▶ host        context) -> allow |
                                                         modify | block | flag
                                   llm-call.wasm ──HTTP──▶ LM Studio / OpenRouter
```

- **Scenario.** A WASM component that owns the world: which agents exist, what
  each one is told, what its actions do, and the record. The kernel knows
  nothing about any particular scenario.
- **Agent.** A WASM component that calls models and tools through host imports.
- **Control.** A WASM component that inspects every action before delivery.
- **Log.** Each run writes `window.SIM_DATA` as a script, so a dashboard reads it
  without a backend.

Principles:

- **One Store per instance.** Nothing is shared except what the host mediates.
- **Minimal capabilities.** Only `llm-call` gets `wasi:http` and the API key.
  Tools, agents, controls and scenarios have no network access.
- **The host mediates every call.** Agents resolve each other's imports through
  the host, and every action passes the control chain.
- **One contract.** `wit/world.wit` defines every interface and world.

## Write your own agent, tool or scenario

- **Scenario:** a crate that targets `wasm32-wasip2` and exports the `scenario`
  interface. `setup`, `begin-day`, `turn-messages`, `action-of`, `context`,
  `record-turn`, `end-day` and `snapshot` drive it. See
  [how the Mars scenario is built](examples/mars-colony/scenario/src/lib.rs).
- **Agent that acts only through tool calls:** the `actor` module, which splits
  an instruction into clauses, keeps the ones addressed to it and asks a
  `tool-llm` for calls. See [`modules/actor`](modules/actor/src/lib.rs).
- **Control:** export the `control` interface. `inspect` decides on each action;
  `describe` says what it enforces. See
  [the Orchestra Director](examples/tab-jam/controls/director-agentWOS/src/lib.rs).
- **Plugin:** the `plugin-module` world, granted only what its manifest names.
  See the [CLI harness](examples/cli-harness/README.md).

## Design

The [design documents](design/README.md) describe where AgentWOS is going: an
operating system for agents, AI first, local first and cloud native, that runs
on top of classic systems and can improve itself. These are proposals, not
implemented yet.

- [Architecture](design/architecture.md): the layers, and the rule that
  everything that runs is an agent and only the kernel is not.
- [Protocol](design/protocol.md): intents, grants, mandates, policy, guards and
  the ledger.
- [Evolution](design/evolution.md): system agents that improve user agents and
  the system itself, and the limits they cannot change.
- [Hot swapping](design/hot-swap.md): replacing agents and workflows while they run.
- [State of the art](design/state-of-the-art.md): published work on
  self-improving agents and what the design takes from it.
- [Benchmark](design/benchmark.md): a plan to measure models on coding tasks and
  build a table for choosing the model of a coding agent per project.
- [Router](design/router.md): a plan to route each piece of work to the best
  expert, using small local decision models and the benchmark table.
- [Roadmap](design/roadmap.md): order of work and open questions.

## Roadmap

1. **Dynamic registry**: `attach(uri, interface)` so an agent loads a tool at
   runtime; the host validates, instantiates and caches it.
2. **One kernel surface**: run `stage` scenarios on the native host and
   `scenario` ones in the browser.
3. **Generic dashboard**: render any scenario's log.
4. **LLM-backed controls**: a control that uses its own model to judge
   consistency across agents.
5. **Evolver**: generate code, compile to WASM, evaluate in a sandbox, promote
   versions in the registry with rollback.

## Troubleshooting

- **`Address already in use` on port 8765**: another server holds the port. Open
  the URL directly or run `tools/serve.py 8766`.
- **`ErrorCode::ConnectionRefused`**: LM Studio's server is not running on `localhost:1234`.
- **`OPENROUTER_API_KEY missing from the host environment`**: export the key
  before using an `openrouter/...` model.
- **`no scenario loaded`**: pass `--manifest` or `--scenario` together with `--colony`.

## Keywords

WebAssembly agents, WASM agents, WASI, Component Model, Wasmtime, agent
operating system, multi-agent orchestration, tool calling, sandboxed agents,
agent isolation, LLM controls and guardrails, browser AI, in-browser LLM,
Needle, jco, Rust agents, agent observability.

## License

Apache License 2.0. See [LICENSE](LICENSE).

Copyright 2026 Ismael Faro Sertage
