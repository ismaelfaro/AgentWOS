# Building the protocol

[Index](../README.md) · [Concept](../protocol.md) · [Stories](../stories/protocol.md)

How to implement intents, the ledger, policy, grants, mandates and guards in the
code that exists today. Code blocks are sketches to show shape; they have not
been compiled.

## What exists to build on

| Today | File | Role in the new design |
|---|---|---|
| `AgentState` implements the `tool` and `llm` imports | `host/src/runtime.rs` | Two of the places an intent is born |
| `PluginState` implements `llm` and `workspace`, with `GRANTS` checked per call | `host/src/harness.rs` | The current, coarse policy check |
| `Plugins::invoke` routes the router's tool call to a plugin | `host/src/harness.rs` | The only agent-to-agent call |
| `ControlChain::apply(author, text, context)` | `host/src/control.rs` | Guards |
| `Log` rewrites one JSON file per run | `host/src/runner.rs` | A ledger for one scenario |
| `inspect`, `cue`, `call` | `web/kernel/kernel.js` | The same path, in the browser |

Every one of these is a separate, hand-written path. The work is to put one
function underneath all of them.

## The one function

```rust
// host/src/kernel.rs
pub struct Kernel {
    parties: Parties,        // name -> labels, instance
    policy: PolicyTable,
    guards: GuardChain,
    ledger: Ledger,
}

impl Kernel {
    /// Every call between parties goes through here.
    pub fn dispatch(&mut self, intent: Intent) -> Result<String, Denied> {
        let decision = self.policy.check(&intent, &self.parties)    // egress, ingress
            .and_then(|_| intent.mandate.check())                   // lending, depth, cycle, budget
            .and_then(|_| self.guards.inspect(&intent));            // allow | modify | block | flag
        let result = match &decision {
            Ok(intent) => self.parties.deliver(intent),
            Err(denied) => Err(denied.clone()),
        };
        self.ledger.append(&intent, &decision, &result);            // always
        result
    }
}
```

The existing host traits then shrink to building an intent and calling
`dispatch`. `LlmHost::complete` becomes an intent with verb `model.complete`;
`WorkspaceHost::exec` one with verb `program.run`; `Plugins::invoke` one with
verb `agent.run`.

## Components to write

| Component | New file | Contents |
|---|---|---|
| Intent | `host/src/intent.rs` | `Intent { id, parent, from, to, verb, body, chain }`; an id counter; a per-agent stack of the intent being served, which gives `parent` |
| Ledger | `host/src/ledger.rs` | Append one JSON line per decision to a file; flush per line |
| Policy | `host/src/policy.rs` | `Selector`, `Rule { peer, verbs, where_, limits }`, `PolicyTable::check` |
| Grants | `host/src/grant.rs` | `Grant { party, verbs, where_ }` and `attenuate`, which returns an error unless the result is a subset |
| Mandate | `host/src/mandate.rs` | `Link { by, lend, budget }`, `Chain::push`, the depth and cycle checks, budget arithmetic |
| Manifest | `host/src/manifest.rs` | One loader for `resources`; translation of today's `grants` into rules |

## Policy matching

Matching has to stay a table lookup. A rule matches when all four hold:

1. The peer selector matches the other party's name or labels.
2. The verb matches one of `verbs`, where `files.*` and `*` are wildcards.
3. Every key in `where` matches the same key in the intent's body, as a glob.
4. The rule's limit, if any, is not exhausted.

An intent is allowed when the sender has a matching egress rule and the receiver
a matching ingress rule. Unknown keys in `where` fail at load time, so a typing
mistake cannot turn into permission.

## Grants as WIT resources

Wasmtime keeps host resources in a `ResourceTable`; a guest only holds an index
the host handed it. That is what makes a grant unforgeable.

```rust
// the guest sees `resource grant`; the host keeps the real thing
impl kernel::HostGrant for AgentState {
    fn attenuate(&mut self, this: Resource<Grant>, verbs: Vec<String>, where_: String)
        -> Result<Resource<Grant>, String>
    {
        let narrower = self.table.get(&this)?.attenuate(&verbs, &where_)?;   // subset or error
        Ok(self.table.push(narrower)?)
    }
}
```

Lending in `kernel.ask` takes `borrow<grant>`, so the handles are valid for that
call and the callee cannot keep them.

## The `agent` world

1. Add `kernel` and `handler` to `wit/world.wit`, and the world `agent`. The
   current interface named `agent` has to be renamed first (for example to
   `runner`), because a world and an interface cannot share a name.
2. Add a `bindgen!` for the new world in `runtime.rs`, next to the existing ones.
3. Keep the old worlds. Their host implementations call `dispatch`, so old and
   new components obey the same policy.

## Guards

`control.inspect(author, text, context)` becomes `guard.inspect(intent-json)`.
To avoid rebuilding every control at once, wrap the old signature: pass
`intent.from` as `author`, the body's main text as `text`, and the scenario
context as before. New guards read the whole intent. The verdict gains
`escalate(party)`.

## The browser kernel

`web/kernel/kernel.js` already has the shape: `inspect` is the guard chain, and
`cue` and `call` are two ways an intent starts. Add a `dispatch` with the same
steps, a policy table loaded from the manifest, and an in-memory ledger the page
can render. Tab Jam's audit log becomes a view of that ledger.

## Order, smallest slice first

1. **Ledger only.** Add `intent.rs` and `ledger.rs`, call `ledger.append` from
   the existing host traits. Behaviour does not change; every call, including
   the ones refused for a missing grant, is now on record.
2. **`dispatch` with today's rules.** Move the `GRANTS` check into
   `PolicyTable`, generated from the manifest's `grants`.
3. **Verbs and `where`.** The harness can now say "`coder` may run `python3` but
   not `cargo`".
4. **Guards in the harness.**
5. **The `agent` world, grants and mandates.**

## How to know it works

| Check | Kind |
|---|---|
| Selector, wildcard and `where` matching, including the unknown-key error | Unit tests on `policy.rs` |
| `attenuate` never returns a wider grant | Property test on `grant.rs` |
| Depth, cycle and budget rules | Unit tests on `mandate.rs` |
| A harness run where `coder` asks for `cargo` gets a denial, and the ledger has the line | Addition to `examples/cli-harness/smoke.sh` |
| The Mars scenario produces the same result as before, plus a ledger | Regression run |
