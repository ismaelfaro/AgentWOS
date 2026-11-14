# Evolution

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/evolution.md) · [stories](stories/evolution.md)

AgentWOS is meant to improve itself, continuously and without being asked: the
agents people run on it, and the system's own agents. This document says how,
and what the improving part is never allowed to change.

## Why this system can do it

Three properties the rest of the design already has make self-improvement
practical instead of reckless:

- **An agent is immutable and content-addressed.** Improving an agent never
  edits it. It produces a new component or new settings with a new digest, and a
  name is moved from the old one to the new one. The old one still exists, so
  going back is always possible.
- **An agent has no authority of its own.** A candidate can be run with no
  grants at all, or with tools that answer from recordings, so trying it cannot
  harm anything.
- **The ledger remembers what happened.** Every intent, verdict, cost and
  confidence is on record. That is both the evidence of what to improve and the
  material to test a candidate against.

## System agents and user agents

Agents carry a tier label. **User agents** do the work people ask for.
**System agents** keep the system running and improving. Both are ordinary
agents under the same protocol; the tier only decides which policies select
them.

The evolution loop is five system agents:

| System agent | Job | Reads | Produces |
|---|---|---|---|
| **Observer** | Finds what is going badly or expensively | The ledger | Opportunities: "agent X fails on Y", "pool Z costs too much" |
| **Scout** | Finds what has appeared outside | Adverts of other realms, component registries, new models | External candidates |
| **Proposer** | Makes a candidate for one opportunity | The agent, its settings, its history | A revision: new settings, a new component, or a different choice of model or expert |
| **Evaluator** | Tests a candidate against the one in use | Replayed intents from the ledger, declared checks | A comparison with evidence |
| **Registry** | Keeps every revision of every name; moves a name forward or back | Evaluations, approvals | Promotions and rollbacks |

## The loop

```
      ledger ──▶ Observer ──┐
                            ├─▶ Proposer ─▶ Evaluator ─▶ Registry ─▶ canary ─▶ promote
 other realms ─▶ Scout ─────┘                  │              │          │
                                               ▼              ▼          ▼
                                            rejected      person      rollback
                                                         approves    if it regresses
```

1. **Observe.** The Observer reads the ledger for signals: failures, intents a
   guard blocked, low confidence, escalations to a person, cost, latency.
2. **Scout.** The Scout looks outside: a new model, a better expert advertised
   by a peered realm, a newer revision of a component the system already uses.
3. **Propose.** The Proposer makes one candidate per opportunity. It may ask a
   worker agent to write a new component, which is compiled to WebAssembly like
   any other.
4. **Evaluate.** The Evaluator runs incumbent and candidate on the same intents,
   replayed from the ledger, with the outside world replaced by recordings.
5. **Canary.** A share of live intents goes to the candidate through a pool. The
   ledger now holds a direct comparison.
6. **Promote or roll back.** The Registry moves the name. If signals worsen
   afterwards, it moves the name back without being asked. Moving a name while
   agents are working is covered in [Hot swapping](hot-swap.md).

Every step is an intent (`ledger.read`, `registry.propose`, `eval.run`,
`registry.promote`, `registry.rollback`), so every step is under policy, within
a budget and in the ledger. The loop has no private channel.

## How candidates are made

The Proposer has a small set of operators, taken from published work on
self-improving agents ([State of the art](state-of-the-art.md)):

| Operator | What it does | Typical target |
|---|---|---|
| **Reflect** | Reads the ledger entries of failed or costly intents, says what went wrong, and changes the agent to address it | Settings, prompts |
| **Mutate** | Makes a small change without a specific diagnosis | Settings, components |
| **Combine** | Takes parts of two revisions that are each good at something | Components, workflows |
| **Adopt** | Takes a candidate the Scout found outside | Models, experts, tools |
| **Distil** | Turns lessons that keep helping ([Hive](hive.md)) into a permanent change | Settings, components |

The Registry keeps an **archive**: every revision with its scores, not only the
current one and the one before. A revision that is not the best overall may be
the best starting point for the next one, and published results show that
searching from a diverse archive does better than always improving the current
best. Cheap operators on cheap targets are tried first, because most of the cost
of the loop is evaluation.

## Inside and outside

| Source | What it finds | Example |
|---|---|---|
| **Inside** | What the system's own history shows | The coder agent fails on tasks with tests; a prompt change fixes most of them |
| **Inside** | Waste | A remote model is used where a local one scores the same |
| **Inside** | Friction | A guard blocks the same harmless intent every day |
| **Outside** | New models | A model that does better on the system's own replayed intents |
| **Outside** | Experts | An agent in a peered realm that outperforms a local one for one verb |
| **Outside** | Newer components | A later revision of a tool the system already runs |
| **Outside** | New tools | A capability the system does not have and keeps being asked for |

Whatever comes from outside is a candidate like any other: untrusted, pinned by
digest, evaluated with no grants, and only from realms a policy admits.

## What can change, and who decides

How far the loop may go on its own is a setting per kind of change, not a
property of the loop.

| What changes | Example | Default mode |
|---|---|---|
| Settings of a user agent | A prompt, which model it prefers | `auto` |
| Component of a user agent | A rewritten worker | `canary` |
| Membership of a pool | Adopting an outside expert | `canary` |
| Settings or component of a system agent | A better Observer | `propose` |
| Budgets | Raising a limit | `propose` |
| Policy | A new egress rule | `propose` |
| Guards | A changed rule in a guard | `propose` |
| The kernel | A change to mediation itself | Never applied by the system |

Modes:

| Mode | Meaning |
|---|---|
| `off` | The loop does not consider this kind of change |
| `propose` | The loop prepares the change and the evidence; a person decides |
| `canary` | The loop may try it on a share of intents and promote it if it holds |
| `auto` | The loop may promote after evaluation alone |

A system that evolves by itself is the goal, and `auto` everywhere is not how to
get there. The modes are meant to be raised one kind of change at a time, as the
ledger shows that the loop's judgment for that kind can be relied on.

## What the loop can never do

These hold whatever the modes say, because the kernel enforces them and the
kernel is outside the loop's reach.

1. **It cannot change the invariants.** Mediation, default deny, attenuation,
   accountability and portability are kernel behaviour, not agents.
2. **It cannot give itself authority.** A revision receives at most the grants
   of the one it replaces. Needing more is a policy change, and policy is not
   `auto`.
3. **No agent promotes its own replacement.** Proposer, Evaluator and Registry
   are different agents with different grants, and a revision of one of them is
   evaluated by the others in their current form.
4. **It cannot remove a guard or reduce what one sees.**
5. **It cannot act on the world while evaluating.** Tools answer from
   recordings or a sandbox.
6. **It spends from its own budget,** separate from the work of user agents, and
   stops when that budget is gone.
7. **Every promotion can be undone,** and is undone automatically if the signals
   it was promoted on get worse.
8. **A person can stop it.** One switch pauses the loop, and a paused loop
   changes nothing.
9. **Everything it does is in the ledger,** including what it rejected and why.

## Improving the system itself

Most of AgentWOS is agents: guards, model agents, tool agents, gateways, and the
five evolution agents. All of them have revisions in the Registry and all of
them can be improved by the same loop, under stricter modes than user agents.
What one agent's improvement means for the others that work with it is covered
in [Hive](hive.md). That is why the kernel is kept small: the smaller the kernel, the more of the
operating system is improvable.

The kernel itself is the one part the system does not change. The loop can
*notice* that the kernel is in the way (the Observer sees it in the ledger) and
can *prepare* a change, but that change goes to people as a proposal with its
evidence. A system that could rewrite the part that enforces its limits would
have no limits.

## What "better" means

A candidate is better only against stated objectives. Each agent, or each pool,
declares them:

```json
"objectives": {
  "checks": ["tests/coder/*.json"],
  "prefer": ["fewer failures", "lower cost", "higher confidence"],
  "never": ["more guard blocks", "more escalations"]
}
```

`checks` are cases with a known good outcome. `prefer` orders candidates that
pass. `never` rejects a candidate that passes but makes something else worse.

A limit to state plainly: the loop improves what it can measure. If the checks
are weak, it will find revisions that pass the checks and are not better. Two
measures reduce this and do not remove it: the Evaluator tests on replayed
intents the Proposer has not seen, and the checks themselves are changed only in
`propose` mode.

## In the manifest

```json
{ "kind": "Evolution", "metadata": { "name": "default" },
  "spec": {
    "paused": false,
    "budget": { "tokens": 200000, "seconds": 3600, "per": "day" },
    "sources": { "ledger": true, "realms": ["lab"], "models": true },
    "modes": {
      "agent.settings": "auto",
      "agent.component": "canary",
      "pool.members": "canary",
      "system.agent": "propose",
      "budget": "propose",
      "policy": "propose",
      "guard": "propose"
    },
    "canary": { "share": 0.1, "minIntents": 50 }
  } }
```

The evolution agents are declared like any others, with `"tier": "system"`, and
policies say what each may do:

```json
{ "kind": "Policy", "metadata": { "name": "evolution-evaluator" },
  "spec": { "selector": { "name": "evaluator" },
            "ingress": [{ "from": { "name": "proposer" }, "verbs": ["eval.run"] }],
            "egress":  [{ "to": { "name": "ledger" }, "verbs": ["ledger.read"] },
                        { "to": { "labels": { "role": "model" } }, "verbs": ["model.complete"] }] } }
```

The Evaluator can read the ledger and use a model. It cannot promote anything,
and it cannot reach a tool.

## Risks

| Risk | What limits it | What remains |
|---|---|---|
| Revisions that pass checks and are worse | Held-out replays; `never` objectives; canary | Weak checks give weak improvement |
| Slow drift away from what people wanted | Every promotion is in the ledger with its evidence; rollback | Someone has to look |
| A bad component from outside | Evaluated with no grants; pinned digest; admitted realms only | A component that behaves well until it is trusted |
| Cost of the loop itself | Its own budget | Tuning that budget |
| The loop improving its own evaluator in its favour | Separation of the three roles; system agents in `propose` | Depends on those modes staying strict |
