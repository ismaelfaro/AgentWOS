# MCP in both directions

AgentWOS talks MCP as a client and as a server, and in both cases through a
**sidecar the OS attaches and watches**. The agent never touches the network or
the protocol.

```
                  ┌──────────── sidecar (per connection) ────────────┐
agent ─ colony.lookup ─►  guard ─► WASM bridge ─► one origin only ─────┼──► outside MCP server
 (grant: mcp:colony)     (built in, fails closed)   token held by kernel │     CLIENT side
                  └───────────────────────────────────────────────────┘

outside MCP client ─► guard ─► WASM bridge ─► exposed agent (`search`, reduced grants)
                                                                          SERVER side
```

## Run it

```bash
rustup target add wasm32-wasip2
examples/mcp/build.sh          # builds the MCP components, the harness and the host
examples/mcp-colony/demo.sh
```

The demo starts `server.py` (a small MCP server, the colony's reference desk) and:

1. **Approval.** Nothing is used until you approve it. `--mcp-approve colony` shows
   the tool definitions and pins their hash in `mcp.lock`; if the server later
   changes a definition, it is quarantined.
2. **Client.** The `desk` agent, which has `mcp:colony`, calls `colony.lookup`.
   The result comes back marked as data, not instructions.
3. **No grant, no access.** The `offline` agent has no `mcp:colony`.
4. **Server.** A stream of JSON-RPC lines, as an outside client would send,
   discovers the system, lists its tools and calls `search`. Only agents under
   `mcp.serve.expose` are offered, and each runs with only the grants listed there.
5. **Status.** `--mcp-call` and `--mcp-status` show what the sidecar did.

## Use it yourself

```bash
python3 examples/mcp-colony/server.py &
host=target/release/agentwos-host; m=examples/mcp-colony/manifest.json
$host --harness $m --mcp-approve colony
$host --harness $m --message '/desk colony.lookup day'
$host --harness $m --mcp-serve        # speak MCP on stdin and stdout
```

To use another MCP server, add it under `mcp.servers` in the manifest, grant
`mcp:<name>` to the agents that may use it, and approve it.

## Files

- `manifest.json`: the MCP server the agents may use, the agents and their grants,
  and what the OS exposes as an MCP server.
- `server.py`: the colony's reference desk, a small MCP server with `lookup` and
  `convert`.
- `demo.sh`: the walk-through above.
- `notes/`: the colony notes (`crew.md`, `power.md`, `water.md`). The `search` agent
  reads them, and it is the one agent an outside MCP client can call.

The test bed that exercises the sidecar with a server that misbehaves is
[examples/mcp](../mcp/).

Limits of this first version: servers over HTTP only (no `command`/stdio servers
yet), serving over stdio only. See [design/mcp.md](../../design/mcp.md).
