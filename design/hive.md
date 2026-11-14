# Hive: planes, shared memory and shared skills

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/hive.md) · [stories](stories/hive.md)

Status: concept under discussion. This document proposes definitions so there
is something concrete to argue with. It is less settled than the others.

## The idea

When one agent learns something that matters to other agents, they should have
it at once, without each of them learning it again. Agents that share this way
behave less like separate programs and more like a hive: what one knows, all
know.

Two things can be shared:

- **Knowledge.** A fact, a correction, a preference, the cause of a failure.
- **Process.** A way of doing something that turned out to work.

## Plane

Sharing needs a boundary, and a realm is the wrong one. A realm is a boundary
of *trust and enforcement*: one kernel and what it vouches for. Sharing follows
a different line: everything that works for one person, one team or one
project, wherever it runs.

A **plane** is that line: a named scope of sharing.

| | Realm | Plane |
|---|---|---|
| Boundary of | Enforcement and trust | Shared knowledge and skills |
| Defined by | A kernel and its key | Who the agents work for |
| Typical size | One machine or deployment | One person, one team, one project |
| Can span | Many planes | Many realms |

A person's laptop kernel and a cloud kernel are two realms. That person's agents
on both are one plane. An agent belongs to one or more planes by label
(`"plane": "ana"`), and a plane can sit inside another (`ana` inside
`team-payments`), so what is learned can be shared narrowly or widely.

## Hive memory

Hive memory is what a plane knows. It is made of **lessons**:

```json
{
  "id": "l-0193",
  "plane": "ana",
  "by": "coder@laptop",
  "about": { "labels": { "writes": "code" }, "topic": "pytest" },
  "lesson": "In this repository tests need PYTHONPATH=src; without it imports fail.",
  "evidence": ["e-0871", "e-0874"],
  "confidence": 0.9,
  "expires": "2027-02-14T00:00:00Z"
}
```

| Field | Why it is there |
|---|---|
| `by` | A lesson always has an author. Anonymous knowledge cannot be weighed or withdrawn |
| `about` | Who it is for, as a selector and a topic. This is what makes it reach the right agents and not all of them |
| `evidence` | The ledger entries it was learned from, so it can be checked |
| `confidence`, `expires` | A lesson is a belief with a strength and a shelf life, not a fact for ever |

Like everything else, hive memory is an agent: a **memory agent** per plane,
serving three verbs.

| Verb | Meaning |
|---|---|
| `memory.publish` | Offer a lesson to the plane |
| `memory.recall` | Ask what the plane knows about a topic, for me |
| `memory.retract` | Withdraw a lesson, by its author or by a person |

"Immediately" then has a precise meaning: a published lesson is returned by the
next `memory.recall` of any agent it is `about`. No agent is interrupted, and
nothing is pushed into an agent's context without it asking. An agent decides
when to recall, typically at the start of a task, and how much to take, because
context is the scarcest thing an agent has.

Because publishing and recalling are intents, they pass through policy and
guards and appear in the ledger like any other. Who may teach whom is a policy
question with the usual answer: nobody, unless a rule says so.

## Hive process

A process that works is shared in the same way, with one more step, because a
process is something agents *do*, not just something they know.

A **skill** is a reusable procedure: the steps, the verbs it uses, when it
applies. There are three ways for a plane to share one, from light to heavy:

| Form | What is shared | Others use it by |
|---|---|---|
| **Recipe** | A description of the steps, as a lesson | Reading it and doing the steps themselves |
| **Service** | The agent that knows how | Asking that agent (`agent.run`), within a mandate |
| **Revision** | A new component or new settings | Running it, after the Registry promotes it for the plane |

The third form is where hive and [Evolution](evolution.md) meet. When one
agent's improvement is promoted in the Registry for a plane, every agent of that
name in the plane runs the improved revision from its next intent. One learns,
all change.

## Fast path and slow path

Lessons and revisions are two speeds of the same thing.

| | Lesson | Revision |
|---|---|---|
| Speed | Immediate | After evaluation |
| Checked by | A guard at publication, readers' own judgment | The Evaluator, on replayed intents |
| Changes | What agents know | What agents are |
| Undone by | Retracting it | Rolling back |
| Risk | Spreads a mistake fast | Slow to help |

The two connect: a lesson that keeps being recalled and keeps helping is an
opportunity for the Observer. The Proposer can turn it into a revision, which
makes it permanent, cheaper (no recall needed) and evaluated. Knowledge starts
soft and fast, and hardens when it proves itself.

## What can go wrong

Immediate sharing spreads a mistake as fast as a discovery. That is the main
design problem of a hive, not a side issue.

| Problem | Proposed answer | Open |
|---|---|---|
| A wrong lesson | Evidence required; confidence carried; a guard on `memory.publish`; retraction is as fast as publication | How much evidence is enough |
| A planted lesson, to steer other agents | Only agents a policy allows may publish; author always known; lessons are data to weigh, never instructions to obey | Models do not reliably keep that distinction |
| A lesson that leaks private data | Planes are the boundary; a lesson never leaves its plane without a mandate | Detecting what is private |
| Too many lessons | `about` selects readers; expiry; recall is bounded by the reader | Ranking what matters |
| Lessons that contradict | Both are returned with authors, evidence and dates | Who resolves it |
| Stale lessons | Expiry; the Observer flags lessons that stop helping | — |

The second row deserves emphasis. A lesson is text written by one agent and read
by another agent's model. That is exactly the shape of a prompt injection. The
protocol can guarantee who wrote a lesson and that a guard saw it; it cannot
guarantee that a reader's model treats it as information. Readers must be built
to weigh lessons, and a lesson must never be able to carry a grant.

## Questions to settle

1. **Is a plane declared or derived?** Declared in the manifest as a kind, or
   simply a label that policies select on? A label is the minimal answer.
2. **Does "immediately" ever mean push?** For a correction that makes running
   work wrong, should the kernel tell agents now instead of at their next
   recall? That would be the first case of the kernel interrupting an agent.
3. **Who may teach?** Any agent in the plane, only agents above a trust label,
   or only after a person confirms the first lessons of a new author?
4. **Where does hive memory live** when a plane spans realms: one memory agent
   that others reach through gateways, or one per realm that synchronise? The
   second survives being offline; the first is simpler.
5. **How does a lesson cross planes?** From a person to their team, or from one
   organisation to a public pool of experts. Proposed: only by an explicit
   mandate, and as a copy with its own author.
6. **Is forgetting a right?** Can a person remove everything a plane learned
   from them, including revisions distilled from their lessons?
7. **What is the unit of a skill?** A recipe is cheap and vague; a revision is
   exact and slow. Is there a useful middle form?
8. **Does private memory exist?** What one agent remembers for itself between
   intents is not designed anywhere yet. Hive memory may be the general case
   and private memory a plane of one.
