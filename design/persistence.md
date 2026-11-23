# Persistent agents: waking, mailboxes, durability and who acts for whom

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [Persistence](persistence.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/persistence.md) · [stories](stories/persistence.md)

Status: proposal. Nothing here is built, except what the "Today" section lists
(the ledger of events and the dashboard agent that reads it).

## Where this comes from

An agent that answers a request and stops is a program. An agent that keeps a
goal over weeks, wakes up on its own, takes notes, works for several people at
once and is still there after a crash is something else, and it needs things a
request-and-answer kernel does not give. A recent LangChain webinar on
"persistent agents" lists four traits: they are **goal oriented**, **always
on**, **accumulate context** and are **multiplayer and multi-surface**. It also
argues that the harness (prompts, tools, loop) is only half of what is needed:
the other half is a runtime around it, and rules that live in the runtime and
not in the prompt. This document takes those ideas and asks where each belongs
in AgentWOS. The answer is mostly the same each time: **in the kernel, as a
small service that an agent is granted, never as something the agent does
for itself.**

## One rule of separation

The kernel owns time, queues, state, identity and the record. Agents own
decisions. Every capability below is a separate piece with one job and one
interface, handed to an agent only when its manifest grants it. This is the
SOLID discipline of object design, applied to a kernel:

| Principle | In AgentWOS |
|---|---|
| Single responsibility | The Clock only fires wakeups. The Mailbox only holds messages. The Journal only records and replays. None of them decides anything about the work |
| Open for extension, closed for change | A new way to wake an agent (a webhook, a file watcher) is a new component that puts messages in a Mailbox. The kernel does not change |
| Substitutability | Any component that implements the `mailbox` interface can stand in for another: a queue in memory, one on disk, one shared by a pool |
| Interface segregation | The grants are narrow and separate: `clock`, `mailbox`, `notes`, `recall`, `observe`. An agent that may schedule itself may not read the ledger |
| Dependency inversion | Agents depend on WIT interfaces, never on how the host implements them. The same agent runs on the native kernel and in a browser |

Two consequences are worth stating. A grant is an *instance*: "A's clock" is
a clock that belongs to A, with limits the kernel set, and A cannot widen
them. And nothing is ambient: an agent with no `clock` grant cannot wake
itself, however it is prompted.

## The parts

### Clock: waking on a schedule

An agent granted `clock` can ask the kernel to wake it later:

- `after(delay, note)` once, `every(period, note)` repeatedly, `cancel(id)`,
  `list()`.
- A wakeup is not a function call. It is a message that arrives in the agent's
  Mailbox from the sender `kernel/clock`, with the reason `schedule` and the
  note the agent left for itself. The agent reads it when it can.
- The kernel keeps the timers, not the agent, so they survive the agent being
  swapped or restarted.
- Limits are the kernel's and are part of the grant: a shortest period, a
  largest number of live timers, and a share of the agent's budget. An agent
  that wakes every second to say nothing runs into them.
- To wake *another* agent an agent needs `clock:<name>`, a separate, narrower
  grant. It is lent like any other and can only be narrowed.

The person or system that created a timer is recorded on it (see "On whose
behalf" below). A timer is a stored promise to act for someone, so it has an
expiry and it is checked again when it fires.

### Mailbox: what happens when someone writes while the agent is busy

This is the actor model, taken as it is: each agent has a **mailbox**, and the
kernel puts messages in it while the agent works. The mailbox is a tool the
agent is granted, not a property of every agent. With `mailbox` an agent can:

- `take(max)` messages when it is ready for them, or `peek` without removing;
- choose a **policy** for each class of sender, within kernel limits:

| Policy | What happens to a message that arrives during a turn |
|---|---|
| `queue` | It waits. The agent takes it after the current turn |
| `steer` | It is shown to the agent at the next safe point of the turn (before its next model call), so the person can redirect work in progress |
| `reject` | The sender gets an answer at once: busy, try later. Nothing is kept |
| `coalesce` | Messages with the same key replace each other (a stream of "status changed" events becomes one) |
| `drop-oldest` | A bounded queue that keeps the latest |

The kernel enforces the capacity and the policy limits. The agent only chooses
among what it was allowed. An agent with no `mailbox` grant has the default:
`reject` while busy, which is what every agent does today.

### Listeners: events that wake an agent

An event source (a new email, a chat message, a file that changed, a webhook)
is a sidecar in the same family as the MCP and A2A sidecars: a component, with
the network access its manifest gives it, that turns something outside into a
message with a sender and a surface. It writes to the Mailboxes of the agents
that subscribed, through the same policy checks as any other message. Adding a
source adds a component and a subscription. It does not touch the kernel.

### Journal: surviving a crash

Every call an agent makes to anything else goes through the kernel. A
WebAssembly component is deterministic except for what the host answers. So the
kernel can record each call and its result in a **journal**, and after a crash
start a new instance and *replay* the journal: the agent re-runs, each call it
makes is answered from the record, and it arrives at the point where it stopped.
Only then do new calls go out for real.

One case needs judgment. A call that was **sent but whose result was never
recorded** may or may not have happened (a payment, an email). The kernel does
not guess. It stops and asks a decision, described below, with the facts: what
the call was, what is known, whether the verb is marked idempotent in the
manifest. The answer is one of: retry, treat as done, abandon, ask a person.

### Principal, surface and on whose behalf

Every intent already says who sends it and to whom. Persistent agents need
three more facts on it:

| Field | Meaning | Example |
|---|---|---|
| `principal` | The person or system this work is for | `ana`, `team-payments`, `system` |
| `surface` | Where the request came from | `dm:ana`, `channel:#ops`, `schedule`, `event:email`, `network` |
| `via` | The chain of agents the intent passed through | `router > coder > git` |

They travel with the intent through every call, are narrowed (never widened) by
a mandate, and are written on every ledger entry. That makes the ledger an
**audit log**: who acted, for whom, from where, through whom, and what the
kernel decided.

A wakeup inherits the principal of whoever created the timer, with the mandate
they held then, attenuated and with an expiry. A schedule must never outlive
the right it was made under.

### Permissions that depend on context

A rule in a prompt is a suggestion. The kernel decides, at the point where a
call crosses a boundary, using `(principal, actor, verb, surface)`:

| Verb | `dm:ana` | `channel:#ops` | `schedule` made by ana |
|---|---|---|---|
| read ana's mail | allow | deny | allow, read only |
| send mail as ana | allow | deny | ask |
| read the team wiki | allow | allow | allow |

The policy is data in the manifest and is checked by the kernel, so a prompt
injection that convinces the model cannot change it. A rule can answer `allow`,
`deny` or `ask`.

### Decider: who answers "ask"

When a rule says `ask`, the kernel does not decide and does not block forever.
It sends the intent, its context and the relevant history to a **decider**, a
system agent whose only job is to resolve asks. The decider returns
`allow`, `deny` or `escalate` (to a person, reached through a surface agent as
any other party), always with a reason. Three limits keep it honest:

- it can only resolve an `ask`; it can never allow what a rule denies;
- every answer is a ledger entry, with the reason;
- it has a budget of asks, and a run of escalations is itself reported.

The same decider answers the Journal's "was it done?" question. Decisions are
made by an agent that can be improved like any other, inside limits it cannot
move.

### Trace data control

The ledger holds a great deal that is sensitive: inputs, outputs, file names,
commands. `observe` is therefore split. The **skeleton** is what happened, who
did it, how long it took and whether it worked, with no content. The **full**
record includes the text. By default a dashboard, and anyone looking at one,
gets the skeleton. The full record needs a separate grant, and can be limited
to entries whose labels match (one principal, one ticket). This is the same
idea as masking inputs and outputs by default and opening them for one case.

## Memory

[Hive](hive.md) says what a plane knows. This section says who writes it.

- **Notes.** An agent granted `notes` can append a line to its own notes while
  it works: what is missing, what is out of date, what it had to find out the
  hard way. Notes are private to the agent and cheap. They are not shared.
- **Curator.** A system agent, woken by the Clock, reads notes and, through
  `observe`, the traces of what agents actually did. It decides what is worth
  keeping and publishes lessons to the plane. This is the "dreaming" agent:
  it runs in the background, so memory is curated rather than dumped. For
  procedural memory (how to do something) traces are the best source; for
  facts, other sources too.
- **Skills are procedural memory.** A skill is a small component or file in a
  known format that an agent loads when it needs it. It is not a special
  mechanism: it is a thing the Registry names and versions like any other.
- **Recall.** An agent granted `recall` can search earlier conversations and
  runs, scoped to the principals it acts for. Whether history is one long
  thread or many does not matter to the agent; the search works across both.

Storing memory is not the hard part. Deciding what to keep, and keeping it
fresh, is. That is why the curator is an agent with a rule set that can be
tested and improved, and not a database feature.

## Improvement

[Evolution](evolution.md) describes the loop. Persistent agents add three
things to it.

- **A failed run becomes a test.** A slice of the ledger (the inputs, the
  journaled answers of every outside call, and what should have happened)
  becomes a **case** that can be replayed against a new revision with the
  outside world frozen. The journal makes this nearly free: the recording
  that allows crash recovery is the same recording that is the fixture.
- **Behaviour is graded, not only answers.** An always-on agent can be
  wrong by doing too much as easily as too little. A case can say *the agent
  should have acted here* or *the agent should have stayed silent*. The
  Evaluator reports both kinds of error: noise (acted when it should not have)
  and misses (stayed silent when it should have acted).
- **Parents propose, children adopt.** Agents have a parent in the manifest
  when one agent exists to supervise others. A parent that watches its
  children's traces proposes a revision; each child takes it at its next safe
  point, by the rules in [Hot swapping](hot-swap.md). The proposal is data, so
  it can be reviewed, tested and refused. The observer and the proposer are
  ordinary components with skills and tools, not special machinery: a coding
  agent pointed at traces is all the improvement loop needs to be.

## Goals

A persistent agent works toward a goal, not a task: "keep the inbox at zero"
instead of "answer this mail". Two design consequences.

- **Goals are written around feedback.** The thing that is hardest to get is a
  person's feedback on what the agent did. A goal that matches how people give
  feedback ("how we handle first outreach", across all accounts) collects more
  of it, and what is learned transfers, than a goal tied to one object.
- **A goal needs a fence.** "Remove all bugs" is satisfied by deleting the
  code. The fence is not in the goal's wording. It is a guard on the verbs that
  can destroy (delete, send, pay), and a budget that makes the cheapest path
  not the most drastic one. The guard sits where the kernel mediates, so it
  holds when the model is wrong.

A wakeup prompt should also carry what the agent did the last time it woke
(from `recall`), so a run every thirty minutes does not repeat itself.

## Seeing it: the dashboard as a process view

The dashboard agent reads the ledger through the read-only `observe` interface.
It already shows each agent's state, its alerts and a live feed, and a graph of
who asks whom. For persistent agents it grows into a process view:

- the graph follows the manifest's parents as well as calls, so a supervisor
  and its children can be navigated;
- each agent shows its Clock timers, its Mailbox depth and policy, and its
  checkpoint age;
- cost per result: spend by agent next to the outcomes it produced;
- controls (pause, resume, drain, kill) are a *separate* grant, `control`.
  Looking and acting are different rights.

## Today

| Built | Where |
|---|---|
| A record of every mediated call, model call, file and program, with who asked whom | `host/src/ledger.rs`, fed from `host/src/activity.rs` |
| A read-only `observe` interface | `wit/world.wit` |
| A dashboard agent: states, alerts, live feed, graph of who asks whom | `modules/dashboard`, served by `host/src/dashboard.rs` |
| Grants, guards, mandates for MCP | `host/src/harness.rs`, `host/src/mcp.rs` |

Not built: Clock, Mailbox, listeners, Journal and replay, principal and surface
on intents, context-dependent policy, the decider, split `observe`, notes,
curator, recall, cases, behaviour evals, parents in the manifest.

## Risks

- **A schedule outlives its permission.** Mitigated by expiry and by checking
  the principal again when a timer fires.
- **An agent wakes itself into a loop.** Mitigated by kernel limits on period,
  count and budget, and by a ledger alert.
- **Replay is wrong if a component is not deterministic.** Components get no
  clock or randomness except through the host; those calls are journaled too.
- **A decider that approves everything.** It can only resolve asks, every
  answer is recorded, and its approval rate is a metric someone watches.
- **The skeleton leaks through metadata.** Names and timings can say a lot.
  Labels let a deployment hide them too.
- **Steering confuses a model mid-thought.** `steer` delivers only at a safe
  point, and as a clearly marked message.
