# AgentWOS

An experimental agent operating system on WASM/WASI. Agents, tools, the LLM
client, OS control modules and scenarios are WASM components. A Rust/Wasmtime
host (Component Model) instantiates them, mediates every call between them and
records what happened. A second kernel runs the same components in the browser.

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
  each one is told, what its actions do, and the record of what happened. The
  kernel knows nothing about any particular scenario.
- **Agent.** A WASM component that calls the LLM and tools through host imports.
- **Control.** A WASM component that inspects every action before it is
  delivered. It can allow, modify, block or flag it. A blocked action is not
  delivered.
- **Log.** Each run writes `window.SIM_DATA` as a script, so a dashboard can read
  it without a backend.

Principles:

- **One Store per instance.** Each component has its own WASI context; nothing is
  shared except what the host mediates.
- **Minimal capabilities.** Only `llm-call` gets `wasi:http` and the API key.
  Tools, agents, controls and scenarios have no network access.
- **The host mediates every call.** Agent imports resolve against other instances
  through the host, and every action goes through the control chain.
- **One contract.** `wit/world.wit` defines every interface and world. The host
  and modules generate their bindings from that file.

## Layout

- `wit/world.wit`: interfaces `types`, `tool`, `llm`, `agent`, `control`, `scenario`,
  and for tool-calling agents `tool-llm`, `actor`, `stage`.
- `host/src/runtime.rs`: WASI, bindings, component loading and instantiation.
- `host/src/control.rs`: control chain.
- `host/src/runner.rs`: scenario runner and log writer.
- `host/src/main.rs`: CLI and manifest loading.
- `modules/agent-core`: agent. `model:<id> <text>` picks the model; `tool:echo <text>` calls the tool.
- `modules/llm-call`: OpenAI-compatible client. `local/<id>` goes to LM Studio
  (`localhost:1234`); `openrouter/<id>` goes to OpenRouter (`OPENROUTER_API_KEY`).
- `modules/tool-echo`: example tool.
- `modules/actor`: agent that acts only through tool calls. Splits an instruction
  into clauses, keeps the ones addressed to it and asks a `tool-llm` for calls.
- `web/kernel/`: browser kernel. Runs components transpiled with `jco`, applies
  the control chain, and provides Needle (a 35 MB tool-calling model) as `tool-llm`.
- `examples/tab-jam/`: browser example: a band of agents you conduct in plain
  English, fully in one tab. See its [README](examples/tab-jam/README.md).
- `examples/mars-colony/`: example scenario: a colony on Mars. Everything it needs
  lives here:
  - `scenario/`: the scenario component.
  - `controls/guardia/`: a control. Blocks sabotage, truncates long messages and
    flags abundance claims while a context value is critical.
  - `build.sh`: builds the scenario and its controls.
  - `manifest.json`: scenario, controls, model, days and log path.
  - `dashboard/index.html`: observation console for this scenario's log.
  - `runs/`: generated logs (git-ignored).

## Requirements

- Rust (rustup) with the `wasm32-wasip2` target: `rustup target add wasm32-wasip2`.
- An OpenAI-compatible LLM backend:
  - **Local**: LM Studio with its server on `localhost:1234` and a model loaded
    (tested with `google/gemma-4-e4b` and `bonsai-1.7b`).
  - **Remote**: OpenRouter, with `OPENROUTER_API_KEY` exported.
- Python 3, only to serve the dashboard.

## Build

```bash
source "$HOME/.cargo/env"
for m in modules/tool-echo modules/llm-call modules/agent-core; do
  (cd $m && cargo build --release --target wasm32-wasip2)
done
cargo build --release

examples/mars-colony/build.sh
```

## Two kernels

| | Native host (`host/`) | Browser kernel (`web/kernel/`) |
|---|---|---|
| Runtime | Wasmtime | The browser's WebAssembly, components transpiled with `jco` |
| LLM | `llm-call` over HTTP (LM Studio, OpenRouter) | Needle, in-tab |
| Drives | `scenario` (turn-based days) | `stage` (interactive, tool calls) |
| Example | `examples/mars-colony` | `examples/tab-jam` |

Both use the same `wit/world.wit` and the same `control` interface. The native
host does not run `stage` scenarios yet, and the browser kernel does not run
`scenario` ones.

## Run a scenario

```bash
./target/release/agentwos-host --manifest examples/mars-colony/manifest.json --colony 3
```

Manifest paths resolve from the manifest's folder, so the example can be moved
or copied as a unit.

A manifest declares what to run:

```json
{
  "scenario": "scenario/target/wasm32-wasip2/release/mars_colony.wasm",
  "controls": ["controls/guardia/target/wasm32-wasip2/release/control_guardia.wasm"],
  "model": "local/google/gemma-4-e4b",
  "days": 10,
  "log": "runs/latest.js"
}
```

Paths resolve from the manifest's folder. CLI flags override manifest values:
`--scenario`, `--control` (repeatable), `--model`, `--colony N` (days), `--log`.

Each turn with a local model takes 20-40 s, so one day with five agents is
roughly two to three minutes.

## Single agent

```bash
./target/release/agentwos-host --message "model:local/google/gemma-4-e4b hello"
./target/release/agentwos-host --message "model:openrouter/anthropic/claude-haiku-4.5 hello"
./target/release/agentwos-host --message "tool:echo hello"
```

Component paths have defaults; override them with `--agent`, `--tool`, `--llm`.

## Writing a scenario

Create a folder with a crate that targets `wasm32-wasip2`, generates bindings
from `wit/world.wit` with the `scenario-module` world, and exports the `scenario`
interface. Keep the scenario's controls, dashboard and runs inside that folder, as
the Mars example does:

- `setup(config)` gets `{model, days}` and returns `{agents: [{agent, role}], ...}`.
  Any other fields are copied into the log for the dashboard.
- `begin-day` and `end-day` bracket each day. `end-day` returns true to stop the run.
- `turn-messages`, `action-of`, `context` and `record-turn` run each agent turn.
  `context` is JSON that every control receives.
- `snapshot` returns the recorded days as JSON.

Then write a manifest next to it that points at the new `.wasm`.

## Log format

The log is `window.SIM_DATA`. Top-level fields come from the run (`model`,
`days_total`, `finished`, `controls`, `start_unix`) and from `setup` (for
example `agents`). `days` is whatever the scenario records. The Mars scenario
records `day`, `event`, `in_progress`, `collapse`, `start_state`, `end_state` and
`turns[]`, where each turn has `agent`, `action`, `delivered`, `zone`, `status`,
`effect`, `controls[]`, `latency_ms`, `format_ok`, `error` and
`resources_after_turn`.

## Dashboard

The Mars dashboard is an example of an observation console for one scenario. Serve
the repo and open it:

```bash
python3 -m http.server 8765 --bind 127.0.0.1
# open http://localhost:8765/examples/mars-colony/dashboard/index.html
```

It refreshes every 3 s while a run is in progress. The cursor (scrubber, `<` `>`,
arrow keys, space to play) moves every panel at once, and "live" follows the
latest turn.

## Troubleshooting

- **`Address already in use` when serving the dashboard**: something is already on
  port 8765. Open the URL directly or use another port.
- **`ErrorCode::ConnectionRefused`**: LM Studio's server is not running on `localhost:1234`.
- **`OPENROUTER_API_KEY missing from the host environment`**: export the key before
  using an `openrouter/...` model.
- **`no scenario loaded`**: pass `--manifest` or `--scenario` together with `--colony`.
- **Empty dashboard**: no `log` path is set, or the first turn has not finished yet.

## Next phases

1. **Dynamic registry**: `attach(uri, interface)` so an agent can load a tool at
   runtime; the host validates the interface, instantiates and caches it.
2. **One kernel surface**: run `stage` scenarios on the native host and `scenario`
   ones in the browser.
3. **Generic dashboard**: render any scenario's log, leaving scenario-specific
   views to scenarios.
4. **LLM-backed controls**: a control that uses its own `llm-call` to judge
   consistency across agents.
5. **Evolver**: generate code, compile to wasm, evaluate in a sandbox and promote
   versions in the registry, with rollback.
