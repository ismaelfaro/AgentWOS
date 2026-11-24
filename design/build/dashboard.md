# Building the dashboard

[Index](../README.md) · [Concept](../dashboard.md) · [Stories](../stories/dashboard.md)

How the first version is built, and where to change it.

## Files

| File | What |
|---|---|
| `wit/world.wit` | `observe` (the import), `dashboard` (the export), `dashboard-module` (the world) |
| `host/src/ledger.rs` | `Ledger`: events, per-agent counters, edges. Unit tests |
| `host/src/activity.rs` | Calls the ledger at every mediated point. `as_agent` says whose work it is; `tracked` records the router's own requests |
| `host/src/dashboard.rs` | Gives the agent `observe` from this kernel's ledger (`Source::Here`) or from another kernel (`Source::Away`), serves its page and JSON |
| `host/src/observe.rs` | The kernel's read-only window: serves the ledger on loopback, writes and removes the note, finds running kernels |
| `host/src/http.rs` | `serve_with`: public paths, loopback `Host` check, content type |
| `modules/dashboard/` | The agent: alerts, JSON, report, and `page.html` |
| `examples/cli-harness/dashboard-smoke.sh` | End-to-end checks, no model needed |

## Running it

```bash
examples/cli-harness/build.sh
target/release/agentwos-host --harness examples/cli-harness/manifest.json --dashboard 127.0.0.1:8830
```

The host prints `dashboard: http://127.0.0.1:8830/#token=...`. Open that link.

On its own, in another terminal, before or after any kernel:

```bash
target/release/agentwos-host --dashboard 127.0.0.1:8830            # the newest kernel, and the next ones
target/release/agentwos-host --dashboard 127.0.0.1:8830 --kernel 4242   # one kernel
```

The window a kernel serves, all `GET`, all with its token:
`/observe/info`, `/observe/now`, `/observe/agents`, `/observe/edges`,
`/observe/events?after=N&limit=M`. `AGENTWOS_RUN_DIR` moves the folder of
notes; the smoke test uses it to keep to itself.
Port `0` picks a free one. With `--message` the host answers and then keeps the
dashboard up until Ctrl-C. With `--serve-http` it watches the agents outside
callers use.

The dashboard component is compiled into the host, like the MCP components, so
it must be built first. `host/build.rs` stops with the command to run if it is
missing, and `cli-harness/build.sh` builds it in order.

## The contract

```wit
interface observe {
    record event { seq: u64, at-ms: u64, agent: string, caller: string,
                   kind: string, detail: string, ms: u64, ok: bool }
    record agent-stat { name: string, busy: u32, calls: u32, errors: u32, thoughts: u32,
                        files: u32, programs: u32, total-ms: u64, busy-since-ms: u64,
                        last-ms: u64, last: string }
    record edge { caller: string, agent: string, calls: u32, busy: u32 }
    now: func() -> u64;
    agents: func() -> list<agent-stat>;
    edges: func() -> list<edge>;
    events: func(after: u64, limit: u32) -> list<event>;
}
```

`events(after, limit)` is a cursor: a reader keeps the last `seq` it saw and
asks for what is newer. Events are trimmed; counters and edges are not.

Event kinds: `start` and `end` (a call to the agent), `think` (a model call),
`file` and `program` (what the agent did while it worked).

## How the ledger is fed

`Activity` already marks every place where the harness shows something: an
agent is asked, a file is read, a model is called, a program runs. Each now
writes an event as well, whether or not a terminal is attached. The agent whose
work it is comes from `Activity::as_agent`, set where `Plugins::invoke` makes
the plugin's own `Activity` and in `instantiate_with` for the first instance.
Callers are `router` (the router's tool calls), `you` (a request typed at the
prompt), `outside` (a caller of `--serve-http`) or a plugin's own name.

## Changing it

- **A new alert**: edit `alerts_of` in `modules/dashboard/src/lib.rs`, rebuild
  the module, then the host (it embeds it).
- **A new view**: edit `modules/dashboard/page.html`. The page polls
  `/api/snapshot?after=N` once a second and keeps no state the server needs.
- **A new thing to count**: add a kind in `Ledger::record`, a field in
  `agent-stat`, and the line that records it in `Activity`. Rebuild everything
  that uses the world: the dashboard module and the host.

## What is not here

- Controls. The agent can only look.
- Tokens and cost. `think` events have a time but not a count of tokens.
- Content. `detail` is a short line (160 characters); there is no full-text
  view and no split into skeleton and full.
- History across restarts. The ledger lives in memory: a monitor that connects
  late sees the latest few thousand events, and a new kernel starts empty.
- More than one kernel at a time. A monitor watches one; `--kernel` picks which.
