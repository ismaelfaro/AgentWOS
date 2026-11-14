# Hot swapping

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/hot-swap.md) · [stories](stories/hot-swap.md)

Continuous improvement only works if a better agent or workflow can replace the
one in use without stopping the system and without breaking work in progress.
This document collects what theory and practice say about that, and proposes
how AgentWOS does it.

## The question

Replacing something that is idle is easy. The hard cases are:

- An agent is in the middle of serving an intent.
- A workflow of many steps and many agents is half done.
- The new revision keeps its state in a different shape from the old one.
- Other agents depend on what the old revision could be asked.

## What theory says: when is a swap safe

Work on dynamic software updating has three answers, each weaker and cheaper
than the one before.

| Condition | Statement | Cost |
|---|---|---|
| **Quiescence** (Kramer and Magee, 1990) | A component may be replaced when it is not involved in any transaction and will not start or serve one until the change is done | Safe, but it has to stop a wide part of the system to get there |
| **Tranquillity** (Vandewoude et al., 2007) | It is enough that the component is not serving a request, has no transaction of its own open, and no neighbour is in the middle of a transaction that has used it and may use it again | Less disruption; still sufficient for consistency |
| **Version consistency** (Ma, Baresi, Ghezzi et al., 2011) | Each transaction, with all its sub-transactions, runs as if entirely on the old configuration or entirely on the new one | Least disruption; needs the system to know which calls belong to which transaction |

The third is the useful one here, and its one requirement is exactly what
AgentWOS has. Every intent goes through the kernel, and every intent carries its
`parent`. The kernel therefore knows the whole tree of intents that one request
caused. In this system a "transaction" is an **intent tree**.

A second line of work (dynamic software updating for running programs) adds two
ideas that carry over: a swap happens only at defined **update points**, and
state moves from old to new through an explicit **transformation**.

## What practice says

| System | How it swaps | What it teaches |
|---|---|---|
| **Erlang/OTP** | Two versions of a module can be loaded at once. A process moves to the new one at its next fully qualified call. Stateful processes convert their state in a `code_change` callback. Loading a third version removes the oldest, and processes still on it are killed | Let old and new run side by side. Make state conversion an explicit step written by the author. Bound how many versions are alive |
| **Temporal** | A workflow can run for months and is recovered by replaying its history, so its code must stay deterministic. A change is either guarded by a patch marker recorded in the history, or shipped as a new workflow type, or handled by pinning executions to worker versions | Work in flight stays on the version it started on unless the change is declared compatible. Replay against recorded history is how compatibility is tested |
| **wasmCloud** | Points a running component at a new image reference; the host fetches, verifies, instantiates and swaps it. Internal state is not carried over | Swapping suits WebAssembly components. Keep state outside the component |
| **Wasmtime** | Has no built-in way to extract the live state of a component instance | Do not design for moving an instance's memory. Design agents that hold no state between intents |
| **Blue-green, canary, rolling** | Old and new run together; traffic moves by share; going back is moving a pointer | The swap is a routing decision, and so is the rollback |
| **Shadow traffic** | The candidate gets copies of real requests and its answers are discarded | A candidate can be measured on real work with no effect |
| **Feature flags** | Behaviour changes by changing a setting, with no new code | Many improvements are settings, and settings swap instantly |

## How AgentWOS swaps

### A name points to a revision

A **revision** is a component digest plus its settings. The Registry
([Evolution](evolution.md)) maps each agent name to its current revision and
keeps the earlier ones. A swap is one atomic change of that pointer. Nothing is
edited, so nothing can be half-changed.

### The rule: an intent tree runs on one configuration

When a root intent starts, the kernel notes which revision each name points to.
Every intent in that tree is served by those revisions, however long the tree
runs. Trees that start after a swap get the new revisions.

This is version consistency, and it costs one table per running tree. It needs
no agent to stop and no agent to cooperate.

### Swap points

An agent keeps no state between intents; what must last lives in a memory agent
or a tool ([Hive](hive.md)). So the moment between two intents is always a safe
update point, and it comes up constantly. This is the design choice that makes
swapping cheap: the hard problem, moving live state, is avoided instead of
solved.

For the cases that do carry state, an agent may serve two optional verbs:

| Verb | Meaning |
|---|---|
| `state.export` | Return my state as data, with a schema version |
| `state.import` | Start from this state; refuse if I do not understand its schema |

These are the counterpart of Erlang's `code_change`. The author of the new
revision writes the conversion, and a revision that cannot import the old state
is simply not eligible for a mid-tree swap.

### Ways to swap

| Way | What happens | Use it for |
|---|---|---|
| `next` | New intent trees use the new revision; running trees finish on the old one | The default |
| `drain` | As `next`, and the old revision is removed once its trees end or a deadline passes | Retiring a revision |
| `shadow` | The new revision receives copies of intents; its results go only to the ledger | Measuring before any exposure |
| `canary` | A share of new trees uses the new revision | Trying on real work, with a small blast radius |
| `resume` | A running tree moves to the new revision at its next step | Long workflows that should not wait; needs declared compatibility |
| `migrate` | A running agent's state is exported and imported into the new revision | Agents that hold state |

`next`, `drain`, `shadow` and `canary` need nothing from the agent. `resume` and
`migrate` need the new revision to declare what it is compatible with.

### Workflows in flight

A workflow is an intent tree whose steps are already in the ledger: each
finished child intent is recorded with its result. That record is a checkpoint.

A new revision of the agent that drives the workflow can take over a running
tree if it declares `"resumes": ["sha256:..."]` for the old revision. On
takeover, steps already in the ledger are not redone; their recorded results are
handed to the new revision, which continues from the first step that is not
there. This is the Temporal idea (recover from recorded history) applied to an
intent tree.

Without that declaration the tree stays on the revision it started with. A long
workflow is then improved from its next run, not this one.

### Compatibility is checked before the swap

A revision states the verbs it serves and their schemas (`handler.describe`).
When a revision is proposed, the Registry compares it with the current one:

- It serves the same verbs, or more, with compatible schemas: any way of
  swapping is allowed.
- It drops or changes a verb: callers that use that verb keep the old revision
  until they have a revision of their own that does not need it. The swap is
  then per caller, not global.

### Never more than a few revisions alive

Old trees pinned to old revisions could keep many revisions alive for ever.
Following Erlang's limit, a name has a bounded number of live revisions. When a
new one would exceed it, the oldest is given a deadline; trees still on it at the
deadline are cancelled, their unused budget returns to whoever lent it, and the
cancellation is in the ledger.

### Rolling back

Rolling back is a swap to an earlier revision, with the same rule and the same
ways. Because the earlier revision was never altered, it behaves exactly as it
did.

### What gets swapped this way

| Thing | Swappable | Note |
|---|---|---|
| Settings of an agent | Yes, instantly | The cheapest improvement |
| A worker, model or tool agent | Yes | The common case |
| A pool's members | Yes | A swap of routing only |
| A guard | Yes, under stricter modes | An intent tree keeps the guards it started with |
| A policy | Yes, under stricter modes | A tree in flight keeps the grants it already holds; narrowing them mid-tree is a cancellation, not a swap |
| The evolution agents | Yes, in `propose` mode | Evaluated by the others in their current form |
| The kernel | No | Replaced by people, with a restart |

### What the ledger adds

Each ledger entry carries the revision of the agent that served it. With two
revisions alive at once, the ledger is a continuous comparison of old against
new on real work, which is what the Evaluator and the automatic rollback read.

## Limits

- **Side effects are not undone.** A swap or a rollback changes what happens
  next. What an old revision already did to the outside world stays done.
- **Stateless is a discipline, not a guarantee.** WebAssembly does not stop an
  agent from keeping state in its memory between intents. An agent that does so
  and does not serve `state.export` loses that state on a swap.
- **Compatibility of meaning is not checkable.** Schemas can match while
  behaviour differs. That is what `shadow` and `canary` are for.
- **`resume` trusts a declaration.** A revision that wrongly claims it can resume
  an older one will continue a workflow incorrectly. Replaying finished trees
  from the ledger through the new revision, before allowing `resume`, is the
  test.

## Open questions

1. How many live revisions per name: two, as in Erlang, or a small configurable
   number?
2. Should `resume` be allowed for workflows that have already acted on the
   outside world, or only for those that have not?
3. Is policy narrowing mid-tree a cancellation (as proposed) or should running
   trees finish under the policy they started with?
4. Should the kernel refuse agents that keep state between intents, by
   instantiating a fresh instance per intent? That makes statelessness a
   guarantee and costs start-up time on every intent.

## Sources

- Vandewoude et al., [Tranquility: A Low Disruptive Alternative to Quiescence for Ensuring Safe Dynamic Updates](https://typeset.io/pdf/tranquility-a-low-disruptive-alternative-to-quiescence-for-a2g4y6r3k5.pdf), IEEE TSE 2007. It also restates Kramer and Magee's quiescence.
- Ghezzi, [Dynamic update: quiescence, tranquillity, version consistency](https://www.slideshare.net/slideshow/laser-4dynamic-update/27774950), LASER 2013 slides.
- Baresi, Ghezzi et al., [version-consistent dynamic reconfiguration](https://re.public.polimi.it/bitstream/11311/1031379/2/11311-1031379_Baresi.pdf).
- Erlang/OTP, [Release Handling](https://www.erlang.org/doc/system/release_handling.md).
- Temporal, [Versioning (Python SDK)](https://docs.temporal.io/develop/python/workflows/versioning) and [Versioning (Go SDK)](https://docs.temporal.io/develop/go/workflows/versioning).
- wasmCloud, [Robust Rollouts and Rollbacks for the Wasm Component Model](https://wasmcloud.website.cncfstack.com/community/2025-02-19-community-meeting/).
- Bytecode Alliance discussion, [cross-instance calls and capturing execution context](https://bytecodealliance.github.io/zulip-archive/stream/206238-general/topic/Cross-instance.20call.20and.20capturing.20full.20execution.20context.html), on why live state extraction is not part of Wasmtime.
- [Versioning and Updating Deployed Wasm Modules](https://salivity.github.io/wasm/article/versioning-and-updating-deployed-wasm-modules), a general article on keeping state outside a module.
