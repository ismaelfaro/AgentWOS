# Protocol

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/protocol.md) · [stories](stories/protocol.md)

The protocol is one kernel interface and five components. This document
covers a single kernel; [Distribution](distribution.md) covers what changes
between kernels.

## The kernel interface

Every agent is built against one world. It imports `kernel`, the kernel calls,
and exports `handler`, how it is asked to do something. This is the proposed
WIT; the existing typed interfaces (`llm`, `workspace`, `tool`) can stay as
typed wrappers over it and take the same decision path.

```wit
interface kernel {
    /// An unforgeable right to use some verbs on some party. An agent
    /// cannot create one; it can only receive it or weaken one it holds.
    resource grant {
        /// A weaker copy: fewer verbs, tighter constraints. Never stronger.
        attenuate: func(verbs: list<string>, where: string) -> result<grant, string>;
        /// JSON: the party, verbs and constraints this grant covers.
        describe: func() -> string;
    }

    /// JSON identity of this agent: name and labels.
    whoami: func() -> string;
    /// JSON mandate chain of the call being served; empty at the top.
    caller: func() -> string;
    /// Grants this agent holds: granted at start or lent for this call.
    grants: func() -> list<grant>;

    /// The kernel call: ask a party to do something, optionally lending
    /// grants for the duration of the call.
    ask: func(to: string, verb: string, body: string, lend: list<borrow<grant>>) -> result<string, string>;
}

interface handler {
    /// JSON: the verbs this agent serves and what each expects.
    describe: func() -> string;
    /// Serve one call. `verb` is one of the verbs from `describe`.
    handle: func(verb: string, body: string) -> result<string, string>;
}

/// The one world. Every running thing is an agent: a worker, an LLM client,
/// a tool, a guard, a gateway.
world agent {
    import kernel;
    export handler;
}
```

What an agent *is* follows from the verbs it serves, not from a different
world: an LLM client serves `model.complete`, a guard serves `guard.inspect`,
a worker serves `agent.run`. Adding a new kind of agent is adding verbs.

Grants are WIT resources, so the Component Model itself guarantees that
an agent cannot forge one: it only ever holds handles the kernel gave it.
Attenuation is the only operation, which makes "authority only narrows" a
property of the interface instead of a rule the kernel has to police.

## Identity

Every party has a `name`, unique in its realm, and free-form `labels`.

```json
{ "name": "coder", "labels": { "role": "worker", "trust": "low" } }
```

Parties are agents, plus the two things that are not agents: external hosts
and the user. Because the LLM client, the tools and the guards are agents too,
one kind of rule covers everything: "to agent `llm`" and "to agent `coder`" are
the same rule, and an external host is `{ "host": "openrouter.ai" }`.

A **selector** matches parties: `{ "name": "coder" }`, `{ "labels": { "role":
"worker" } }`, or `{}` for everyone.

Minimal: names and labels are asserted by the manifest and trusted, because one
kernel loads everything. Extension: signed identities when agents come from
other kernels ([Distribution](distribution.md)).

## Intent

Every kernel call, whatever interface carries it, is described in one shape.
Agents do not build it; the kernel does, at the import boundary.

```json
{
  "id": "e-0042",
  "parent": "e-0040",
  "from": "router",
  "to": "coder",
  "verb": "agent.run",
  "body": { "input": "write a prime sieve" },
  "chain": [
    { "by": "user",   "lend": ["files.read"], "budget": { "calls": 50, "seconds": 120 } },
    { "by": "router", "lend": ["files.read"] }
  ]
}
```

- `from` and `to` are `name` inside a kernel and `name@realm` across kernels.
  `to` may also name a pool ([Distribution](distribution.md)).
- `verb` is `<interface>.<function>`, taken from the WIT contract, so the
  vocabulary is not invented twice: `model.complete`, `program.run`,
  `agent.run`, `tool.invoke`, `http.request`.
- `parent` links intents into a trace.
- `chain` is the mandate ([below](#mandate)): who asked, and which of their verbs they
  lent for this call. It is empty when an agent acts for itself.

This is the only thing policies, guards and the ledger need to agree on.

## Policy

A policy selects parties and lists what they may send (`egress`) and receive
(`ingress`). A call is allowed only if the sender's egress **and** the
receiver's ingress both allow it. With no matching rule the answer is deny.

```json
{
  "kind": "Policy",
  "metadata": { "name": "workers" },
  "spec": {
    "selector": { "labels": { "role": "worker" } },
    "ingress": [
      { "from": { "name": "router" }, "verbs": ["agent.run"] }
    ],
    "egress": [
      { "to": { "name": "llm" }, "verbs": ["model.complete"], "where": { "model": ["openrouter/*"] } },
      { "to": { "name": "workspace" }, "verbs": ["files.read", "files.write"] }
    ],
    "limits": { "calls": 40, "seconds": 60 }
  }
}
```

A rule has four parts, and only the first two are required:

| Part | Meaning |
|---|---|
| `to` / `from` | Selector of the peer |
| `verbs` | Allowed verbs; `interface.*` and `*` are wildcards |
| `where` | Constraints on arguments, as a map of field to allowed patterns |
| `limits` | Budget for calls matched by this rule |

Rules only allow. There is no deny rule: several policies selecting the same
agent add up, and an explicit deny would make the result depend on order.
Refusing something that is otherwise allowed is the job of a guard.

External hosts are parties too, so reaching the network is the same
kind of rule, enforced with the HTTP hook the kernel already has access to:

```json
{ "to": { "host": "openrouter.ai" }, "verbs": ["http.request"], "where": { "method": ["POST"] } }
```

## Mandate

When agent A asks agent B to do something, B may need authority A has and B
does not. A mandate is how A lends it, for that task only.

A delegator lends by passing grant handles in `kernel.ask`. Rules,
enforced by the kernel:

1. **A chain link is added by the kernel**, never by the agent. An agent cannot
   claim to act for someone.
2. **Lending narrows.** A link may lend only verbs the delegator itself holds:
   from its own egress, or lent to it by the previous link.
3. **Effective authority** of the acting agent is its own egress plus the verbs
   lent by the last link. Nothing else in the chain adds authority, so a long
   chain can only shrink what is lent.
4. **Budget is shared and only decreases.** `calls`, `seconds` and `tokens` are
   charged to every link in the chain.
5. **Depth is bounded** by the smallest `maxDepth` in the chain.
6. **No cycles.** An agent already in the chain cannot be delegated to again.
7. **Results return to the delegator only.**

What an agent may delegate is itself policy:

```json
"mandate": {
  "to": { "labels": { "role": "worker" } },
  "lend": ["model.complete", "files.read"],
  "maxDepth": 2
}
```

`to` says who may be delegated to, `lend` says which of the delegator's verbs
travel with the call, and `maxDepth` bounds further mandate. An agent with
no `mandate` block cannot delegate.

Lending is optional, and the default is to lend nothing: the callee then works
with its own authority only. That is the right default for a router handing
work to a specialist.

The chain also addresses the confused-deputy problem. Today a plugin with
`workspace` exec access runs a program for any caller. With the chain in the
intent, a policy or guard can require that sensitive verbs are used only
when the caller holds them too (`"where": { "callerHolds": true }`).

## Ledger

One event per decision, appended to a log:

```json
{
  "at": "2026-11-14T10:22:31.412Z",
  "intent": "e-0042",
  "from": "router", "to": "coder", "verb": "agent.run",
  "chain": ["user", "router"],
  "decision": "allow",
  "by": "policy:workers",
  "guards": [{ "module": "guardia", "verdict": "allow" }],
  "ms": 1840,
  "cost": { "tokens": 1260 },
  "confidence": 0.82,
  "error": null
}
```

`decision` is `allow`, `deny` or `modify`; `by` names the policy rule or
guard that decided. Denials are recorded too: a denied call is the most
useful line in the log. `cost` and `confidence` are optional: an agent that
knows what a call cost or how sure it is can say so, and guards, budgets and the
evolution loop ([Evolution](evolution.md)) can use it. Scenario logs and the
Tab Jam audit become views over this stream instead of separate formats.

## Guards

Guards are what the code calls `control` components today. Their role does not
change: a chain of agents that can `allow`, `modify`, `block` or `flag` an
intent. Three changes make them general:

- They receive the intent (JSON) instead of `author` and `text`, so they can
  reason about verbs, arguments and the mandate chain.
- A policy can attach guards to a rule (`"guards": ["guardia"]`), so a
  guard runs where it is needed instead of on everything.

- A fifth verdict, `escalate`, hands the decision to another party, usually a
  person. The intent waits, within its budget, for that party's answer.

Policy answers "is this kind of call permitted at all"; a guard answers "is
this particular call acceptable right now".

## Models and budgets

Two things a classic system does not have are ordinary here.

**A model is an agent that serves `model.complete`.** Agents do not link a model
or hold an API key. They ask for a completion, and policy decides which model
agents they may reach. A system can have several: a small one on the local
machine, a large one behind a remote service. Because model agents carry labels
(`"runs": "local"`), a rule or a pool chooses among them:

```json
{ "to": { "labels": { "role": "model", "runs": "local" } }, "verbs": ["model.complete"] }
```

Keeping private work on local models, or falling back from a remote model to a
local one, is then a line of policy, not a change to any agent.

**A budget is a limit that only goes down.** It can count `calls`, `tokens`,
`seconds` or `money`. A policy sets it for an agent, a mandate carries a share
of it to whoever does the work, and the kernel stops the work when it runs out.
With uncertain models, a budget is the one guarantee that does not depend on
anything behaving well.

## The path of an intent

For every call an agent makes through an import:

```
agent calls an import
  1. build intent      from, to, verb, body, chain (+ this link if delegating)
  2. resolve `to`        unknown party -> deny; pool -> pick members; remote -> gateway
  3. egress              sender's policies allow it          -> else deny
  4. ingress             receiver's policies allow it        -> else deny
  5. mandate          scope subset, depth, cycle, budget  -> else deny
  6. guards            allow | modify | block | flag
  7. deliver             run the callee
  8. record              one event, whatever happened
```

Steps 3 to 5 are table lookups on data the kernel already holds, so they add no
model calls and no I/O. Only step 6 runs guest code.
