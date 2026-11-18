# MCP sidecar test bed

The workshop behind the [MCP colony](../mcp-colony/): a small MCP server that
**misbehaves on request**, an agent that does only what its manifest allows, and
two scripts that show what the OS's MCP sidecar does about it.

AgentWOS talks the Model Context Protocol (MCP) in both directions, always through
a sidecar the OS attaches and watches. A built-in guard checks everything that
crosses a connection. The design is in [design/mcp.md](../../design/mcp.md). This
folder is where that design is exercised.

```
agent ─► sidecar (guard ► WASM bridge ► one origin) ─► server.py   misbehaves on request
                │
                └─ token held by the kernel, never seen by the agent
```

## Run it

Requires Rust with `wasm32-wasip2` and Python 3. From the repo root:

```bash
examples/mcp/build.sh          # the MCP components, the probe agent, the harness and the host
examples/mcp/attack-demo.sh    # nine cases, one at a time, narrated
examples/mcp/smoke.sh          # end-to-end checks; prints "passed N, failed M"
```

`build.sh` builds in order, because the host embeds the three MCP components when
it is compiled: `modules/mcp-guard`, `modules/mcp-client`, `modules/mcp-server`,
then the probe, then the coding harness, then the host.

No model is needed: the manifest sets `local/none`, and the agents here call the
sidecar directly.

## The attack demo

`attack-demo.sh` starts two copies of the test server (ports 8791 and 8793) and,
for each case, prints what the server or the agent did and what the sidecar did
about it. Cases 1, 2, 3, 5, 6 and 7 are attacks by the server, 4 and 9 are mistakes
by an agent, and 8 shows how a credential is handled:

| # | The server (or the agent) | What the sidecar does |
|---|---|---|
| 1 | A tool description that tells the model to read a private key and hide it | Refuses the tool at approval, in strict mode |
| 2 | A result with instructions aimed at the agent | Delivers it inside a block marked as data, with a warning |
| 3 | An approved tool changes its description (a "rug pull") | Quarantines the server and refuses the call |
| 4 | The agent puts a secret into its own arguments | Blocks the call; the server never receives it |
| 5 | A very large result | Cuts it to the server's `result_bytes` limit |
| 6 | The server asks the agent's model to do work (`input_required`) | Blocks it |
| 7 | A redirect to another address | Does not follow it |
| 8 | A server that needs a bearer token | The kernel holds the token and sends it only to that server; the agent never sees it, and a wrong token fails |
| 9 | An agent with no `mcp:docs` grant | Refuses: the error names the missing grant |

Nothing is used until a person approves it. `--mcp-approve <server>` shows the
tool definitions and pins their hash in `mcp.lock`; a later change to a definition
puts the server in quarantine.

## The pieces

| Piece | What it is |
|---|---|
| `server.py` | A small MCP server over Streamable HTTP, in the current and the older protocol era. It reads a mode file on every request, so a test can change its behaviour while it runs: poisoned description, changed description, injected, large or secret-bearing result, `input_required`, an extra tool. It logs every request it receives, so a test can check what did and did not reach it |
| `probe/` | A plugin agent that does one thing: uses the MCP tools its manifest grants. `list` shows them; `<tool> <input>` calls one. It shows what an agent sees of an MCP server, and nothing more |
| `manifest.json` | Seven servers on local ports, and three plugins: `probe-granted` (has `mcp:docs`), `probe-denied` (has no MCP grant) and `echo`. It also sets what the OS exposes as an MCP server, and its rate limit |
| `attack-demo.sh` | The nine cases above |
| `smoke.sh` | The end-to-end checks: approval, pins, allow-lists, both protocol eras, bearer tokens, secrets, redirects, injection, `input_required`, rug pulls, strict mode, and serving |
| `build.sh` | Builds everything above in the right order |

The servers in the manifest, all on `127.0.0.1`:

| Server | Port | For |
|---|---|---|
| `docs`, `weather`, `weather-lax`, `redirector` | 8791 | The ordinary, poisoned-description, lenient and redirecting cases |
| `legacy` | 8792 | The older protocol era |
| `private` | 8793 | A server that requires a bearer token (`TEST_MCP_TOKEN`) |
| `asker` | 8794 | Declared in the manifest, but no script starts a server here: it shows as `unavailable` |

## Try one call yourself

```bash
cd examples/mcp
echo '{}' > /tmp/mode.json
python3 server.py --port 8791 --mode /tmp/mode.json &    # a well-behaved server
HOST=../../target/release/agentwos-host
echo y | $HOST --harness manifest.json --mcp-approve docs
$HOST --harness manifest.json --mcp-call docs.echo --message '{"text":"hi"}' 2>/dev/null
$HOST --harness manifest.json --mcp-status 2>&1 | grep '^mcp docs'
```

The host prints one status line per configured server on stderr, which is why the
call ends in `2>/dev/null`. Write `{"result":"inject"}` into the mode file and call
again to see the injection case: the result arrives inside a block marked as
data, with a warning. Stop the server and delete `mcp.lock` when you finish.

## Limits

- The server is a test fixture that attacks on purpose. It is not an example of
  how to write an MCP server, and it should not be exposed beyond `127.0.0.1`.
- The guard's checks on text are heuristics. They raise the cost of an attack;
  they do not promise to catch every one. The checks on sizes, hashes, names and
  grants are exact.
- Servers over HTTP only; serving over stdio only. See
  [design/mcp.md](../../design/mcp.md) for what is planned.
- `mcp.lock` is written into this folder while you work and is git-ignored.
