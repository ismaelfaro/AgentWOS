# Building persistent agents

[Index](../README.md) · [Concept](../persistence.md) · [Stories](../stories/persistence.md)

How to add the Clock, the Mailbox, the Journal, principal and surface, context
policy and the decider to the code that exists. The first slice, the ledger and
the dashboard, is built. The rest is the plan, in an order where each step is
useful alone.

## What exists to build on

| Today | File | Use |
|---|---|---|
| `Ledger`: events, per-agent counters, caller to agent edges | `host/src/ledger.rs` | Grows `principal`, `surface` and `via`; becomes the audit log |
| `Activity` calls the ledger at every mediated point | `host/src/activity.rs` | The one place that sees every call. The Journal hooks in beside it |
| `Plugins::invoke`, `run_exposed` | `host/src/harness.rs` | Where an intent is made, so where principal and surface are attached |
| `PluginState` host imports, checked per grant | `host/src/harness.rs` | The pattern for new grants (`GRANTS`, `Scope::of_grants`) |
| `observe` and the dashboard agent | `wit/world.wit`, `modules/dashboard`, `host/src/dashboard.rs` | The model for a component that gets one narrow import |
| MCP guard chain with checkpoints | `host/src/mcp.rs`, `modules/mcp-guard` | The model for the context policy and the decider |
| `http::serve_with` on loopback | `host/src/http.rs` | Control endpoints for the dashboard |

## New contracts

Each is its own interface and its own grant. None imports another.

```wit
interface clock {
    use types.{call-error};
    /// Ids are the kernel's. Limits come from the grant, not from the caller.
    after: func(delay-ms: u64, note: string) -> result<u64, call-error>;
    every: func(period-ms: u64, note: string) -> result<u64, call-error>;
    cancel: func(id: u64) -> result<_, call-error>;
    list: func() -> list<timer>;
    record timer { id: u64, next-in-ms: u64, period-ms: u64, note: string }
}

interface mailbox {
    use types.{call-error};
    record envelope {
        id: u64,
        from: string,        // an agent, "kernel/clock", "you", "outside", an event source
        surface: string,
        principal: string,
        reason: string,      // "message", "schedule", "event"
        body: string,
    }
    enum policy { queue, steer, reject, coalesce, drop-oldest }
    take: func(max: u32) -> list<envelope>;
    peek: func(max: u32) -> list<envelope>;
    /// Per class of sender: "person", "agent", "schedule", "event:<source>".
    set-policy: func(class: string, policy: policy) -> result<_, call-error>;
}

interface notes {
    use types.{call-error};
    append: func(text: string) -> result<_, call-error>;
    read: func() -> list<string>;
}
```

`recall` and a split `observe` follow the same shape. `observe` becomes
`observe-skeleton` and `observe-full`: the dashboard component gets only the
first by default, and the second only when its manifest says so.

## The intent gets three fields

```rust
pub struct Intent { /* today: sender, target, verb, body */
    pub principal: String,   // who the work is for
    pub surface: String,     // dm:ana, channel:#ops, schedule, event:email, network
    pub via: Vec<String>,    // router > coder > git
}
```

Where they are set:

- `Requests` in `prompt.rs` and `--message` in `harness.rs`: principal from
  the manifest (`principal`), surface `cli`.
- `serve_http` in `harness.rs`: principal from the authenticated caller,
  surface `network`.
- A timer firing: principal and mandate stored on the timer when it was made.
- `Plugins::invoke`: `via` gets the caller pushed, principal and surface are
  copied. A callee never sets its own.

`Activity::note` already takes `(agent, caller, kind, detail, took, ok)`. Replace
the argument list with the intent and the ledger event gains the three fields
for free. `AgentStat` and `Edge` stay as they are.

## The Clock

`host/src/clock.rs`: one thread, one ordered map of `(due, id) -> Timer`. A
timer holds `{ agent, principal, mandate, expires, period, note }`.

- On firing: check `expires` and the principal's mandate again. If either fails,
  drop the timer and write a ledger entry saying why.
- Then `mailbox.offer(agent, Envelope { from: "kernel/clock", reason: "schedule", .. })`.
- The grant carries limits: `{"clock": {"min_period_s": 60, "max_timers": 4}}`.
  `after` and `every` check them before they store anything.
- State is serialisable (a JSON file next to the manifest) so timers survive a
  restart of the kernel. A timer that fell due while the kernel was down fires
  once on start, not once per missed period.

## The Mailbox

`host/src/mailbox.rs`: one `Mailbox` per agent that has the grant, behind
`Arc<Mutex<..>>`, so a sender thread can `offer` while the agent's thread runs.

```rust
pub enum Offer { Queued, Steered, Rejected(&'static str), Coalesced }

impl Mailbox {
    pub fn offer(&self, env: Envelope) -> Offer;       // applies the policy for env's class
    pub fn take(&self, max: usize) -> Vec<Envelope>;
}
```

`steer` is the one with a subtlety: the harness loop calls `mailbox.steered()`
before each model call and, if there is anything, appends it to the messages as a
message marked `[arrived while you were working]`. That needs one line in
`LlmHost::complete` for plugins with the grant, and nothing else.

An agent without the grant behaves as today: `Plugins::invoke` runs when the
instance is free, and a second call to a busy agent gets another instance. Only
an agent that wants a single serialised stream asks for `mailbox`.

## Listeners

Event sources are components under a new world `source-module` that export
`poll: func() -> list<event>` and import only what their manifest grants (an
HTTP host, or a file path). The kernel calls `poll` on a timer and offers each
event to the subscribers. Polling is the simple start; push sources can follow
with the same envelope.

## The Journal

`host/src/journal.rs`. The idea: every host import that an agent calls is
wrapped once, in one place, so that the call and its result are recorded, and
in replay mode the result is returned from the record.

```rust
pub struct Journal { path: PathBuf, entries: Vec<Entry>, cursor: Option<usize> }
pub enum Entry { Called { seq: u64, call: String }, Returned { seq: u64, result: Vec<u8> } }

impl Journal {
    pub fn call<T: Serialize + DeserializeOwned>(
        &mut self, name: &str, args: &impl Serialize, real: impl FnOnce() -> T,
    ) -> T {
        if let Some(recorded) = self.replaying(name, args) { return recorded; }
        self.log_called(name, args);
        let out = real();
        self.log_returned(&out);
        out
    }
}
```

The `Host` impls on `PluginState` (`complete`, `read`, `write`, `exec`,
`invoke`) each become one line through `journal.call`. Time and randomness
reach the component only through WASI imports, which the host owns, so those
two are journaled as well and replay is exact.

On start the kernel scans for a journal with `Called` entries that have no
`Returned`. Those are **in doubt**. For each one it sends the decider:

```json
{ "question": "in-doubt-call", "agent": "mail", "call": "invoke send {...}",
  "idempotent": false, "journal_tail": ["..."] }
```

and acts on `retry`, `done`, `abandon` or `ask-person`.

Compact the journal at every turn boundary the agent reports (a `done` event on
its mailbox), so it is bounded by one turn, not by the agent's life.

## Policy by context

Extend the manifest with rules evaluated in `Plugins::invoke` and the MCP
checkpoints, before the call:

```json
"policy": [
  { "verb": "mail.read", "surface": "dm:*",       "do": "allow" },
  { "verb": "mail.read", "surface": "channel:*",  "do": "deny",  "why": "others can read the thread" },
  { "verb": "mail.send", "surface": "schedule",   "do": "ask" }
]
```

First match wins, default deny. The evaluator is a plain function in
`host/src/policy.rs` (not a model), so it is testable with a table. `ask`
produces a call to the decider.

## The decider

A system agent in `plugin-module` form (so the existing harness loads it) with
these constraints set by the kernel, not by the agent:

- its only import is a `decide` request with the intent, context and history;
- its answers are parsed into `allow | deny | escalate`, anything else is
  `deny`;
- an answer for a verb the policy denies is discarded;
- it has a budget of asks per hour; over it, `escalate`.

`escalate` goes to a person through a surface agent that is just another
plugin with a `channel` it may write to.

## Cases and behaviour evals

A **case** is a JSON file written from a ledger slice and its journal:

```json
{ "inputs": [ { "from": "you", "body": "..." } ],
  "fixtures": [ { "call": "read inbox", "returns": "..." } ],
  "expect": { "acts": true, "verbs": ["mail.archive"], "silent_when": ["newsletter"] } }
```

`agentwos-host --harness m.json --replay case.json` instantiates the agents with
the journal in replay mode against the fixtures, then checks `expect`. Two
numbers come out: how often it acted when it should have stayed silent (noise)
and how often it stayed silent when it should have acted (misses).

## Parents in the manifest

A plugin may have `"parent": "<name>"`. The kernel does not give a parent power
over a child. It records the tree (and `Plugins::invoke` rejects a cycle), and
the dashboard draws it. What a parent may do to a child is a grant on the
Registry: `propose:<child>`.

## The dashboard

- Graph: edges today come from calls; add `parent` edges with a different
  style. The data is already in `observe.edges`.
- Process table: one row per agent with state, timers, mailbox depth and last
  checkpoint. Needs `observe` to gain `timers` and `mailboxes` (small lists).
- Cost per result: needs tokens or money on a `think` event; `llm-call` can
  return usage and the host writes it to the ledger entry.
- Controls: a second endpoint family, `POST /api/control` with `pause`,
  `resume`, `drain`, `kill`, behind its own token and only when started with
  `--dashboard-control`.

## Order of work

1. **Principal, surface and via on the intent and the ledger.** No behaviour
   change. Everything after it can be audited.
2. **Split `observe` into skeleton and full.** A dashboard defaults to the
   skeleton.
3. **Mailbox** with `queue` and `reject`, then `steer`. Smoke: send while busy.
4. **Clock**, with limits, expiry, and persistence. Smoke: an agent that
   schedules itself and a timer that is refused.
5. **Policy by context** and the decider. Smoke: the same verb allowed in
   `dm:ana`, denied in `channel:#ops`, `ask` on `schedule`.
6. **Journal and replay.** Smoke: kill the kernel mid-turn, restart, finish.
7. **Notes, curator, recall.** The curator is a plugin on a timer.
8. **Cases, replay and behaviour evals.**
9. **Parents and proposals**, with the Registry from [Hot swapping](../hot-swap.md).

Steps 1 to 3 need no new idea and unblock the rest. Step 6 is the biggest.

## Tests

- `ledger`: unit tests for the new fields; a run through `Plugins::invoke`
  shows `principal` and `via` on every event.
- `mailbox` and `clock`: pure Rust unit tests on the policy table and the due
  order; no WASM needed.
- `policy`: a table test of rules against `(verb, surface)`.
- `journal`: record a run of the `echo` and `files` plugins, replay, and compare
  the ledger.
- Examples: an example under `examples/` for each story, with a smoke script
  that needs no model.
