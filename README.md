# AgentModules

Experimental replica of DeepAgents (LangChain) on WASM/WASI. Agents, tools, the
LLM client and OS control modules are independent WASM components, orchestrated
by a Rust/Wasmtime host (Component Model).

## Architecture

```
                      host (Rust, Wasmtime)  =  "Agentic WASM OS"
  ┌───────────────────────────────────────────────────────────────────┐
  │ colony: world state, events, turns, zone effects, log             │
  │                                                                   │
  │  colonist 1..N (AgentInstance, own Store)                         │
  │    agent-core.wasm ──import tool──▶ host ──▶ tool-echo.wasm       │
  │                    ──import llm───▶ host ──▶ llm-call.wasm ──HTTP─┼─▶ LM Studio / OpenRouter
  │                                                                   │
  │  ControlChain: control-*.wasm (own Store each)                    │
  │    inspect(author, text, resources) -> allow|modify|block|flag    │
  └───────────────────────────────────────────────────────────────────┘
          │ sim/runs/latest.js (rewritten after every turn)
          ▼
     dashboard/index.html
```

Principles:

- **One Store per instance.** Each component has its own WASI context; nothing
  is shared except what the host mediates.
- **Minimal capabilities.** Only `llm-call` gets `wasi:http` and the API key.
  Tools, agents and controls have no network access.
- **The host mediates every message.** Agent imports resolve against other
  instances through the host, and every colonist action goes through the control
  chain before it is delivered. A blocked action is not executed and has no
  effect on resources.
- **One contract.** `wit/world.wit` defines every interface and world; the host
  and modules generate their bindings from that file.

## Layout

- `wit/world.wit`: interfaces `types`, `tool`, `llm`, `agent`, `control`, `scenario` and their worlds.
- `host/src/runtime.rs`: WASI, bindings, component loading and instantiation.
- `host/src/control.rs`: control module chain.
- `host/src/runner.rs`: generic scenario runner and dashboard log.
- `host/src/main.rs`: CLI and manifest loading.
- `examples/mars-colony/`: the Mars colony as a scenario module plus its `manifest.json`.
- `modules/agent-core`: agent. `model:<id> <text>` picks the model; `tool:echo <text>` calls the tool.
- `modules/llm-call`: OpenAI-compatible client. `local/<id>` goes to LM Studio
  (`localhost:1234`), `openrouter/<id>` to OpenRouter (`OPENROUTER_API_KEY`).
- `modules/tool-echo`: example tool.
- `modules/control-guardia`: deterministic control. Blocks sabotage, truncates
  long messages and flags abundance claims while a context value is critical.
- `dashboard/index.html`: observation console.

## Requirements

- Rust (rustup) with the `wasm32-wasip2` target: `rustup target add wasm32-wasip2`.
- An OpenAI-compatible LLM backend:
  - **Local**: LM Studio with its server running on `localhost:1234` and a model
    loaded (tested with `google/gemma-4-e4b` and `bonsai-1.7b`).
  - **Remote**: OpenRouter, with `OPENROUTER_API_KEY` exported.
- Python 3, only to serve the dashboard.

## Quick start

```bash
source "$HOME/.cargo/env"
for m in tool-echo llm-call agent-core control-guardia; do
  (cd modules/$m && cargo build --release --target wasm32-wasip2)
done
cargo build --release

(cd examples/mars-colony/scenario && cargo build --release --target wasm32-wasip2)

./target/release/agentmodules-host --manifest examples/mars-colony/manifest.json --colony 3

python3 -m http.server 8765 --bind 127.0.0.1
# open http://localhost:8765/dashboard/index.html
```

With gemma-4-e4b locally each turn takes 20-40 s: one day with 5 colonists is
roughly 2-3 minutes.

## Single agent

```bash
./target/release/agentmodules-host --message "model:local/google/gemma-4-e4b hello"
./target/release/agentmodules-host --message "model:openrouter/anthropic/claude-haiku-4.5 hello"
./target/release/agentmodules-host --message "tool:echo hello"
```

The `.wasm` paths have defaults; override them with `--agent`, `--tool`, `--llm`
and `--control` (repeatable).

## Scenarios and manifests

A scenario is a WASM component that owns the world: which agents exist, what
they are told each turn, what their actions do, and the record of what happened.
The kernel only runs the turns, sends each action through the control chain and
writes the log. Mars is one scenario; the kernel knows nothing about it.

`examples/mars-colony/manifest.json`:

```json
{
  "scenario": "scenario/target/wasm32-wasip2/release/mars_colony.wasm",
  "controls": ["../../modules/control-guardia/target/wasm32-wasip2/release/control_guardia.wasm"],
  "model": "local/google/gemma-4-e4b",
  "days": 10,
  "log": "../../sim/runs/latest.js"
}
```

Paths resolve from the manifest's folder. CLI flags override manifest values:
`--scenario`, `--control` (repeatable), `--model`, `--colony N` (days), `--log`.

Run a different scenario by writing a new `scenario` crate that exports the
`scenario` interface, and a manifest that points to it.

The Mars scenario replies in JSON (`action`, `zone`, `status`). Working in a zone
has an effect on resources, on top of daily consumption and events.

### Log format

`sim/runs/latest.js` defines `window.SIM_DATA`. It loads as a script, so the
dashboard works without a backend. Main fields:

- `model`, `days_total`, `finished`, `controls`, `colonists`, `zones`, `initial`.
- `days[]`: `day`, `event`, `in_progress`, `collapse`, `start_state`, `end_state`, `turns[]`.
- `turns[]`: `agent`, `action`, `delivered` (text after controls, `null` if
  blocked), `zone`, `status`, `effect`, `controls[]` (`module`, `verdict`,
  `detail`), `latency_ms`, `format_ok`, `error`, `resources_after_turn`.

`sim/runs/` is in `.gitignore`.

## Dashboard

Serve the repo with `python3 -m http.server 8765 --bind 127.0.0.1`, then open
`http://localhost:8765/dashboard/index.html`. It refreshes every 3 s while the
simulation runs. It shows:

- KPIs: day, turns, blocks, warnings, average latency, % of valid JSON replies.
- Base map and colonist table at the cursor position.
- Resources turn by turn.
- OS audit: control verdicts, errors and replies outside the expected format.
- Day turns with filters and search.

The cursor (scrubber, `<` `>`, arrow keys, space to play) moves every panel at
once. "Live" follows the latest turn.

## Troubleshooting

- **`OSError: [Errno 48] Address already in use` when serving the dashboard**:
  something is already listening on 8765 (for example the server Claude Code
  starts from `.claude/launch.json`). Open the URL directly or use another port
  (`python3 -m http.server 8766 --bind 127.0.0.1`).
- **`ErrorCode::ConnectionRefused`**: LM Studio's server is not running on
  `localhost:1234`.
- **`OPENROUTER_API_KEY missing from the host environment`**: export the key before
  using an `openrouter/...` model.
- **`no scenario loaded`**: pass `--manifest` or `--scenario` together with `--colony`.
- **Empty dashboard**: the host was not started with `--log sim/runs/latest.js`,
  or the first day has not finished any turn yet. The dashboard also works opened
  as `file://`, but serving it over HTTP is more reliable.

## Next phases

1. **Dynamic registry**: `attach(uri, interface)` so an agent can load a new tool
   at runtime; the host validates the interface, instantiates and caches it.
2. **LLM-backed controls**: a control that uses its own `llm-call` to judge
   consistency between colonists.
3. **Evolver**: generate code, compile to wasm, evaluate in a sandbox and promote
   versions in the registry, with rollback.
