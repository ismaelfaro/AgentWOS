# Building evolution

[Index](../README.md) · [Concept](../evolution.md) · [Stories](../stories/evolution.md)

How to implement the evolution loop: the five system agents, the modes, and the
limits. It assumes the ledger from [Building the protocol](protocol.md) and the
Registry from [Building hot swapping](hot-swap.md). Code blocks are sketches;
they have not been compiled.

## What each agent needs from the kernel

| Agent | Verbs it uses | Verbs it serves |
|---|---|---|
| Observer | `ledger.read` | `observer.opportunities` |
| Scout | `gateway.send` (adverts of peers), `http.request` to named registries | `scout.candidates` |
| Proposer | `registry.read`, `ledger.read`, `model.complete`, `agent.run` (to ask a coder agent) | `proposer.propose` |
| Evaluator | `ledger.read`, `model.complete`, `eval.instantiate` | `eval.run` |
| Registry | (kernel built-in at first) | `registry.read`, `registry.propose`, `registry.promote`, `registry.rollback` |

Two of these are kernel services exposed as verbs, because they touch what only
the kernel has:

- **`ledger.read`**: a filtered read of the ledger (by agent, verb, time,
  decision). Policy decides who may read whose entries.
- **`eval.instantiate`**: start a revision in a dry-run tree with no grants, and
  deliver intents to it. This is the same mechanism as `shadow`.

## What the ledger must hold

Replaying an intent needs its inputs and the results it got back. The ledger
described in the protocol holds decisions. For evaluation it also needs, per
intent, the body and the result.

| Option | Trade-off |
|---|---|
| Store bodies and results for every intent | Complete; large; may hold private data |
| Store them only for agents with `objectives` declared | Enough for evolution; bounded |
| Store a hash, and keep the content in a separate store with its own expiry | Ledger stays small; content can be forgotten |

Start with the second. Make it a setting per agent (`"record": "full"`), off by
default.

## Observer, without a model first

The first Observer is arithmetic over the ledger, grouped by agent and verb:

```rust
struct Signal {
    agent: String, verb: String,
    intents: u32, failures: u32, blocked: u32, escalated: u32,
    mean_confidence: f32, tokens: u64, p95_ms: u64,
}
```

It emits an opportunity when a rate crosses a threshold, with the ids of the
intents behind it. A model is useful later, to name *why* a group of intents
failed. It is not needed to notice that they did.

## Proposer: the Reflect operator

```
input:  the revision's settings, and N ledger entries the Observer pointed at
        (body, result, error, guard verdicts)
prompt: "Here is how this agent is configured and N cases where it failed.
         Say what the cases have in common. Propose changed settings that
         address it. Change as little as possible."
output: new settings  ->  registry.propose(name, parent_digest, settings)
```

A proposal is a new entry in the name's archive with `parent` set. It changes
nothing that runs.

The other operators come later and reuse the same output: `Mutate` skips the
diagnosis, `Combine` takes two parents, `Adopt` wraps a Scout candidate, and a
component-level proposal asks a coder agent for source, compiles it for
`wasm32-wasip2`, and proposes the resulting digest.

## Evaluator

```
eval.run(name, candidate, incumbent):
  cases   = checks declared in objectives  +  replayable intents from the ledger
            that the Proposer was not given
  for each case, for both revisions:
      instantiate in a dry-run tree; deliver; record result, cost, guard verdicts
  score   = objectives.prefer, in order
  reject  if any objectives.never got worse
  return  comparison, with the case ids
```

Two details keep it honest:

- **Held-out cases.** The Evaluator picks cases; the Proposer's policy does not
  let it read the Evaluator's selection or the `checks` files.
- **The Evaluator has no way to promote.** Its policy has no egress rule to
  `registry.promote`.

## Registry: modes and promotion

The Registry applies the `Evolution` resource. For each proposal it knows the
kind of change (settings, component, system agent, policy, guard) and its mode:

| Mode | What the Registry does with a passing evaluation |
|---|---|
| `off` | Refuses the proposal |
| `propose` | Emits an intent to a person (`human.approve`) with the diff and the evidence, and waits |
| `canary` | Starts a canary; promotes when `minIntents` have run and no `never` objective regressed |
| `auto` | Promotes |

After a promotion the Registry keeps watching the signals the Observer computes.
If they are worse than the evaluation predicted, over a minimum number of
intents, it rolls back and records why.

## The limits, in code

| Limit | Where it is enforced |
|---|---|
| A revision gets at most the grants of the one it replaces | The Registry swaps a pointer; grants are attached to the *name* by policy, so a new revision cannot bring any |
| No agent promotes its own replacement | `registry.promote` refuses when the proposer or evaluator of a proposal is the name being changed |
| Guards cannot be removed | The Registry has no verb for removing an agent, only for moving a name between revisions |
| Evaluation cannot act on the world | Dry-run trees: tool agents refuse or answer from recordings |
| Separate budget | The `Evolution` resource's budget is the mandate under which every evolution intent runs |
| A person can stop it | `paused: true` makes the Registry refuse `propose` and `promote` |
| The kernel is out of reach | The kernel is not a name in the Registry |

## The first slice

Enough to see the loop turn once, on a laptop:

1. `kernel.settings`, and the coder agent's prompt moved into settings.
2. `"record": "full"` for the coder agent.
3. An Observer that reports the coder's failure rate.
4. A Proposer with `Reflect`.
5. An Evaluator over five checks in `examples/cli-harness/checks/`.
6. Mode `propose`: the Registry prints the settings diff and the comparison, and
   a person types yes or no.

No canary, no Scout, no component changes. Everything after is widening this.

## How to know it works

| Check | Kind |
|---|---|
| A deliberately bad prompt is replaced by a better one that passes more checks | End-to-end, with a person approving |
| A candidate that passes checks but triggers more guard blocks is rejected | Integration test with a `never` objective |
| The Proposer's attempt to read the checks folder is denied and is in the ledger | Policy test |
| A promoted revision that does worse on live intents is rolled back | Integration test with a scripted regression |
| With `paused: true`, nothing changes | Integration test |
| Evolution intents stop when its budget is spent | Integration test |
