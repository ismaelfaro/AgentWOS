# Building A2A support

[Index](../README.md) · [Concept](../a2a.md) · [Stories](../stories/a2a.md) · [MCP build](mcp.md)

What was built and where. It reuses the MCP sidecar almost entirely.

## Pieces

| Piece | Where | What |
|---|---|---|
| `a2a-client` | `modules/a2a-client/` (WASM, world `mcp-client-module`) | Reads the card, turns skills into tool definitions, sends `SendMessage`, follows `GetTask`, turns parts into text. Same interface as `mcp-client`, so the kernel treats both alike |
| `protocol` field | `host/src/mcp.rs`, `ServerConfig` | `"mcp"` (default) or `"a2a"`; picks which bridge component a server uses |
| `a2a.rs` | `host/src/a2a.rs` | The card and the task methods for serving: data translation, no agent is run here. Unit tested |
| `http.rs` | `host/src/http.rs` | Loopback HTTP/1.1 server with token, caps and chunked bodies; unit tested |
| `serve_http` | `host/src/harness.rs` | Routes `/a2a`, `/mcp` and the card into the same `call_agent` that stdio uses |
| `confined` | `host/src/workspace.rs` | For outside callers, refuses program arguments that leave the workspace |
| `--serve-http ADDR` | `host/src/main.rs` | Starts HTTP serving |

## Manifest

Calling out:

```json
{ "name": "weather", "protocol": "a2a", "url": "http://127.0.0.1:8811",
  "auth": { "bearer_env": "WEATHER_TOKEN" }, "tools": ["forecast", "plan"] }
```

`url` is the base of the agent: the card is read from `<url>/.well-known/agent-card.json`.
`tools` is the list of skill ids agents may use. Skill ids become tool names; a
character that is not a letter, digit, `_`, `-` or `.` becomes `_`.

Serving:

```json
"serve": {
  "expose": [{ "plugin": "search", "grants": ["workspace"] }],
  "auth": { "bearer_env": "AGENTWOS_SERVE_TOKEN" }
}
```

```bash
AGENTWOS_SERVE_TOKEN=<16 or more characters> agentwos-host --harness serve.json --serve-http 127.0.0.1:8820
```

## How a call goes

1. `list-tools` reads the card and returns one definition per skill. The
   definition carries the agent's name, version, interface and the skill id, so
   they are part of what `mcp.lock` pins.
2. A call becomes `SendMessage` with the text in one part and the skill in
   `metadata.skill`, `returnImmediately: false`, header `A2A-Version: 1.0`.
3. The reply is read as a task or a message, in the current shape or the older
   one. While the task is `submitted` or `working`, the bridge calls `GetTask`
   once a second, up to 20 times.
4. `completed` returns the artifact and status text; `input-required` and
   `auth-required` return a note with the task id; `failed`, `canceled` and
   `rejected` return an error result. A task still running after the polls
   is returned as an error that names its id.
5. The result then passes the same result checkpoint as any MCP result.

## What it took

- The bridge: about 270 lines of Rust.
- The host: a protocol field, one more component to load, and the approval
  message. Supervision, pins, limits, status, the token and the egress hook needed
  no change.
- Serving: `a2a.rs`, `http.rs` and `serve_http`, plus the confinement fix below.

## A finding while building it

Exposing the `search` agent over HTTP showed that an outside caller could make it
read any file: `search root /etc/passwd` ran `grep root /etc/passwd`, because the
workspace checked which program ran and not what arguments it was given. That was
true of `--mcp-serve` too. The fix is `Workspace::confined`, used only for
outside callers: an argument that is absolute, starts with `~`, climbs with `..`,
or has such a value after `=`, is refused. Local agents are unchanged.

## How to know it works

```bash
examples/mcp/build.sh
examples/a2a/smoke.sh      # 33 checks
```

The smoke script starts a test A2A agent that misbehaves on request
(`examples/a2a/agent.py`), a second one that needs a token, and an AgentWOS
serving over HTTP, then calls each from another AgentWOS.

## Order for what comes next

1. A real task table: a worker per task, `returnImmediately`, `CancelTask` that cancels, `SubscribeToTask`.
2. Verify card signatures where present, as an extra signal beside the pin.
3. Per-caller tokens, written to the Ledger.
4. Fuzz `http.rs`.
5. Test against other implementations of A2A.
