# A2A: agents calling agents

AgentWOS speaks A2A (Agent2Agent, v1.0, JSON-RPC binding) in both directions,
through the same sidecar as [MCP](../mcp/): a remote agent's skills appear to
local agents as tools, and exposed agents are offered to outside callers with an
Agent Card. Concept: [design/a2a.md](../../design/a2a.md).

## Run it

```bash
rustup target add wasm32-wasip2
examples/mcp/build.sh            # builds every component, the harness and the host
examples/a2a/demo.sh             # AgentWOS serving, AgentWOS calling it, an attack
examples/a2a/smoke.sh            # 33 checks, including attacks
```

## What is here

| File | What |
|---|---|
| `agent.py` | A small A2A agent for testing: a quick skill, one that asks for input, one that takes time. It can also misbehave (changed skills, a poisoned description, injected text, a card pointing elsewhere) |
| `manifest.json` | The calling side: `weather` and `peer` as A2A servers (`"protocol": "a2a"`), and a plugin allowed to use them |
| `serve.json` | The serving side: exposes `search` over the colony notes, with a token |
| `demo.sh`, `smoke.sh` | The walkthrough and the checks |

## Use it yourself

Call a remote A2A agent: add it to `mcp.servers` with `"protocol": "a2a"`, grant
`mcp:<name>` to the agents that may use it, and approve it.

```bash
python3 examples/a2a/agent.py &
host=target/release/agentwos-host; m=examples/a2a/manifest.json
$host --harness $m --mcp-approve weather
$host --harness $m --mcp-call weather.forecast --message '{"message":"Jezero crater"}'
```

Offer your agents: list them under `mcp.serve.expose` and give a token.

```bash
AGENTWOS_SERVE_TOKEN=<16+ characters> $host --harness examples/a2a/serve.json --serve-http 127.0.0.1:8820
```

It serves the card at `/.well-known/agent-card.json`, A2A at `/a2a` and MCP at
`/mcp`, on loopback only, and nothing is answered without the token.

Limits of this version: no streaming, push notifications or signed cards; a served
task finishes inside its request. Both sides were tested against each other and
against test agents written from the specification, not yet against other
implementations.
