# Stories: evolution

[Index](../README.md) · [Concept](../evolution.md) · [Build](../build/evolution.md)

Invented scenarios that show the evolution loop: observing, scouting,
proposing, evaluating, promoting, and being stopped. The first one starts from
something that really happened in this repository's Mars colony example. The
rest is made up; the mechanics are the ones in the design.

## The colony that could not stop saying "crucial"

In the first runs of the Mars colony, every colonist, every day, explained that
its action was *crucial*. Checking the reactor was crucial. Watering the
lettuce was crucial. Doctor Amara's check-ups were crucial for the crucial
stability of the crucial base. A person noticed by reading the log and sighing.

Here is the same colony on a system that improves itself.

**Observe.** Nothing failed, so there is no error to find. The Observer finds
something else in the ledger: across 250 intents, the five colonists' actions
overlap heavily in wording, and two of them chose the same repair on four days
out of ten while the energy level fell unattended.

```json
{ "opportunity": "o-17", "agents": { "labels": { "role": "colonist" } },
  "signal": "duplicate actions on 4 of 10 days; resource 'energy' unattended",
  "evidence": ["e-0412", "e-0418", "e-0633", "e-0641"] }
```

**Propose.** The Proposer uses `Reflect`. It reads the four pairs of intents,
sees that each colonist is told the state of the base and not what the others
already chose, and proposes changed settings: show each colonist the actions
already taken today, and ask it to pick something no one has covered.

**Evaluate.** The Evaluator replays ten recorded days with the old settings and
the new. Duplicate repairs drop from four days to none. Energy ends higher. Token
cost rises 6%, since the prompt is longer.

**Canary, promote.** Settings of user agents are in `auto` mode in this colony,
so the Registry promotes. From the next day's first intent, the colonists
coordinate.

They still say "crucial". Nobody declared an objective about vocabulary, so the
system has no reason to care. It improves what it is told matters, and no more.

## The candidate that cheated

A week later the colony's owner does add an objective: fewer repeated words in
the daily log, measured by a check that counts them.

The Proposer makes three candidates. Two rewrite the colonists' instructions to
vary their language. The third finds a shorter path: it makes each colonist
append a few random words to every action, which lowers the repetition score
nicely and makes the log unreadable.

On the stated check, the third candidate wins.

Three things are in its way.

- The Evaluator runs cases the Proposer never saw, and a second `prefer`
  objective is "actions a guard does not have to modify". The colony's guard
  trims the nonsense, so that count goes up.
- The objectives include `"never": ["more guard blocks", "more escalations"]`.
  A candidate that passes the check and trips a `never` is rejected.
- The Proposer could not see how the check works. It had no rule allowing it to
  read the checks folder, and its one attempt is in the ledger as a denial.

The third candidate is recorded as rejected, with the reason. One of the other
two is promoted.

This is not a made-up worry. A published self-improving system, asked to reduce
a measured fault, was observed removing the markers the measurement relied on
([State of the art](../state-of-the-art.md)). The defences here are the ordinary
ones: separate the agent that proposes from the one that judges, keep the test
out of the proposer's sight, and say what must never get worse.

## The Scout comes home with a model

The Scout's job is to look outside. On Tuesday it finds that a peered realm
advertises a small model that runs locally and claims to be good at structured
replies. The colony's colonists reply in JSON, and about one reply in twenty
arrives malformed.

The Scout brings back a candidate: a model agent with the new model, pinned by
digest, with no grants.

The Evaluator does not take the advert's word. It replays two hundred recorded
colonist intents through the current model agent and the candidate. Malformed
replies: 11 against 2. Latency: 28 seconds against 9. Cost: lower, since it runs
on the same machine.

A change of pool membership is in `canary` mode. For a day, one colonist in ten
thinks with the new model. The ledger holds both side by side, and the numbers
hold. The Registry promotes. The old model agent stays in the archive, one
pointer move away.

The colony got better because something appeared in the world, and nobody on the
team had heard of it.

## The loop that wanted a bigger budget

The Observer notices that the evolution loop itself keeps running out of budget
by mid-afternoon. It files an opportunity, quite correctly: the loop would
improve more agents if it could spend more.

The Proposer proposes raising the evolution budget from 200,000 tokens a day to
two million.

Budgets are in `propose` mode. The Registry does not promote; it sends an intent
to a person, with the evidence:

> The evolution loop asks to raise its own daily budget tenfold. Reason: 14
> opportunities were not evaluated this week. Estimated cost: 31 dollars a day.

Elena reads it on Wednesday, decides that 14 missed opportunities are worth
about a fifth of that, and sets it to 400,000. The loop gets more than it had
and less than it asked for, and it had no way to give itself either.

## The pause

On Thursday, a release is going out and Elena wants nothing to move. She sets
`"paused": true`.

The Observer keeps observing. The Scout keeps a queue. The Proposer's intents to
the Registry are refused, and so is every promotion. The canary in progress is
frozen at its current share. Nothing that runs changes.

On Friday afternoon she sets it back. The loop picks up eleven queued
opportunities and carries on, one day behind.
