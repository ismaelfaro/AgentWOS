# Protocol

[Index](README.md) · [OS model](os-model.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

The protocol is one system call interface and five components. This document
covers a single kernel; [Distribution](distribution.md) covers what changes
between kernels.

## The system call interface

Every party imports the same small interface. This is the proposed WIT; the
existing typed interfaces (`llm`, `workspace`, `tool`) stay as they are for
convenience and take the same decision path.

```wit
interface kernel {
    /// An unforgeable right to use some verbs on some party. A component
    /// cannot create one; it can only receive it or weaken one it holds.
    resource capability {
        /// A weaker copy: fewer verbs, tighter constraints. Never stronger.
        attenuate: func(verbs: list<string>, where: string) -> result<capability, string>;
        /// JSON: the party, verbs and constraints this capability covers.
        describe: func() -> string;
    }

    /// JSON identity of this process: name and labels.
    whoami: func() -> string;
    /// JSON delegation chain of the call being served; empty at the top.
    caller: func() -> string;
    /// Capabilities this process holds: granted at start or lent for this call.
    capabilities: func() -> list<capability>;

    /// The system call: ask a party to do something, optionally lending
    /// capabilities for the duration of the call.
    call: func(to: string, verb: string, body: string, lend: list<borrow<capability>>) -> result<string, string>;
}

/// Any agent, tool or service.
world process {
    import kernel;
    export agent;
}
```

Capabilities are WIT resources, so the Component Model itself guarantees that a
component cannot forge one: it only ever holds handles the kernel gave it.
Attenuation is the only operation, which makes "authority only narrows" a
property of the interface instead of a rule the kernel has to police.

## Identity

Every party has a `name`, unique in its realm, and free-form `labels`.

```json
{ "name": "coder", "labels": { "role": "worker", "trust": "low" } }
```

Parties are: agents, services (LLM, workspace, a tool), controls, external
endpoints (a host name) and the user. One namespace for all of them keeps rules
uniform: "to the LLM service" and "to agent `coder`" are the same kind of rule.

A **selector** matches parties: `{ "name": "coder" }`, `{ "labels": { "role":
"worker" } }`, or `{}` for everyone.

Minimal: names and labels are asserted by the manifest and trusted, because one
kernel loads everything. Extension: signed identities when agents come from
other kernels ([Distribution](distribution.md)).

## Envelope

Every system call, whatever interface carries it, is described in one shape.
Processes do not build it; the kernel does, at the import boundary.

```json
{
  "id": "e-0042",
  "parent": "e-0040",
  "from": "router",
  "to": "coder",
  "verb": "agent.run",
  "body": { "input": "write a prime sieve" },
  "chain": [
    { "by": "user",   "lend": ["workspace.read"], "budget": { "calls": 50, "seconds": 120 } },
    { "by": "router", "lend": ["workspace.read"] }
  ]
}
```

- `from` and `to` are `name` inside a kernel and `name@realm` across kernels.
  `to` may also name a pool ([Distribution](distribution.md)).
- `verb` is `<interface>.<function>`, taken from the WIT contract, so the
  vocabulary is not invented twice: `llm.complete`, `workspace.exec`,
  `agent.run`, `tool.invoke`, `http.request`.
- `parent` links envelopes into a trace.
- `chain` is the delegation ([below](#delegation)): who asked, and which of their verbs they
  lent for this call. It is empty when an agent acts for itself.

This is the only thing policies, controls and the record need to agree on.

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
      { "to": { "name": "llm" }, "verbs": ["llm.complete"], "where": { "model": ["openrouter/*"] } },
      { "to": { "name": "workspace" }, "verbs": ["workspace.read", "workspace.write"] }
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
Refusing something that is otherwise allowed is the job of a control.

External destinations are parties too, so reaching the network is the same
kind of rule, enforced with the HTTP hook the kernel already has access to:

```json
{ "to": { "name": "openrouter.ai" }, "verbs": ["http.request"], "where": { "method": ["POST"] } }
```

## Delegation

When agent A asks agent B to do something, B may need authority A has and B
does not. Delegation is how A lends it, for that call only.

A delegator lends by passing capability handles in `kernel.call`. Rules,
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
"delegation": {
  "to": { "labels": { "role": "worker" } },
  "lend": ["llm.complete", "workspace.read"],
  "maxDepth": 2
}
```

`to` says who may be delegated to, `lend` says which of the delegator's verbs
travel with the call, and `maxDepth` bounds further delegation. An agent with
no `delegation` block cannot delegate.

Lending is optional, and the default is to lend nothing: the callee then works
with its own authority only. That is the right default for a router handing
work to a specialist.

The chain also addresses the confused-deputy problem. Today a plugin with
`workspace` exec access runs a program for any caller. With the chain in the
envelope, a policy or control can require that sensitive verbs are used only
when the caller holds them too (`"where": { "callerHolds": true }`).

## Record

One event per decision, appended to a log:

```json
{
  "at": "2026-11-14T10:22:31.412Z",
  "envelope": "e-0042",
  "from": "router", "to": "coder", "verb": "agent.run",
  "chain": ["user", "router"],
  "decision": "allow",
  "by": "policy:workers",
  "controls": [{ "module": "guardia", "verdict": "allow" }],
  "ms": 1840,
  "error": null
}
```

`decision` is `allow`, `deny` or `modify`; `by` names the policy rule or
control that decided. Denials are recorded too: a denied call is the most
useful line in the log. Scenario logs and the Tab Jam audit become views over
this stream instead of separate formats.

## Controls

Unchanged in role: a chain of WASM components that can `allow`, `modify`,
`block` or `flag`. Two changes make them general:

- They receive the envelope (JSON) instead of `author` and `text`, so they can
  reason about verbs, arguments and the delegation chain.
- A policy can attach controls to a rule (`"controls": ["guardia"]`), so a
  control runs where it is needed instead of on everything.

Policy answers "is this kind of call permitted at all"; a control answers "is
this particular call acceptable right now".

## The system call path

For every call a process makes through an import:

```
process calls an import
  1. build envelope      from, to, verb, body, chain (+ this link if delegating)
  2. resolve `to`        unknown party -> deny; pool -> pick members; remote -> gateway
  3. egress              sender's policies allow it          -> else deny
  4. ingress             receiver's policies allow it        -> else deny
  5. delegation          scope subset, depth, cycle, budget  -> else deny
  6. controls            allow | modify | block | flag
  7. deliver             run the callee
  8. record              one event, whatever happened
```

Steps 3 to 5 are table lookups on data the kernel already holds, so they add no
model calls and no I/O. Only step 6 runs guest code.
