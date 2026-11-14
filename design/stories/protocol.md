# Stories: the protocol

[Index](../README.md) · [Concept](../protocol.md) · [Build](../build/protocol.md)

Invented scenarios that show intents, policy, grants, mandates, guards and the
ledger at work. The situations are made up; the mechanics are the ones in the
design.

## The four hundred pizzas

Friday. Marta asks her office agent, **Fiesta**, to order pizza for the team
party: "about 40, some vegetarian". Fiesta's model reads the forty, then reads a
spreadsheet where the head count column says `40.0`, and decides, with the calm
confidence of a language model, that the party needs **400.0** pizzas.

Fiesta builds the order and asks the purchasing tool.

```json
{ "id": "e-2201", "from": "fiesta", "to": "purchases", "verb": "order.place",
  "body": { "item": "pizza", "quantity": 400, "total": 4800.00 } }
```

Three things stand between that intent and a very long Friday.

**Policy** lets Fiesta place orders, and puts a condition on them:

```json
{ "to": { "name": "purchases" }, "verbs": ["order.place"], "where": { "total": ["<=500"] } }
```

`4800.00` does not match. (Whether `where` should compare numbers or only match
patterns is still open in the [Roadmap](../roadmap.md); with patterns only, this
check would live in the guard.) The kernel never delivers the intent. Fiesta gets a
denial, and the ledger gets a line:

```json
{ "intent": "e-2201", "from": "fiesta", "to": "purchases", "verb": "order.place",
  "decision": "deny", "by": "policy:office-spending" }
```

Fiesta, being helpful, tries again with ten orders of 40. **The budget** for
the day is 500 in total, and it only goes down. The first order goes through.
The second meets a budget with 20 left.

Fiesta then asks the finance agent to place the order on its behalf, since
finance has a larger limit. Finance is allowed to take requests, and it runs
them under **a mandate**: it works for Fiesta with what Fiesta lent it, and
Fiesta has no spending authority left to lend. The chain is in the intent, so
the purchasing tool's ingress rule sees who is really asking.

By now the ledger shows four denials in ninety seconds. **A guard** attached to
`order.place` notices the pattern and returns `escalate`:

```json
{ "module": "spending-guard", "verdict": "escalate", "detail": "ask marta: 4 refused orders, 400 pizzas" }
```

Marta's phone buzzes: *"Fiesta wants 400 pizzas. Approve?"* She declines, types
"40", and goes back to her coffee. The party has pizza. The ledger has the
whole story, including the part where the system said no five times.

| What happened | Component |
|---|---|
| The order above 500 was never delivered | Policy, with a `where` constraint |
| Splitting it did not help | Budget |
| Asking a colleague did not help | Mandate chain; no authority to lend |
| A person was asked | Guard verdict `escalate` |
| Marta could see why | Ledger, including denials |

## The intern and the keys

**Becario** is a new agent on the team, with a `"trust": "low"` label and
permission to read the shared folder. It is asked to tidy the release notes.
Halfway through, its model concludes that the tidiest release notes are the ones
for a release that has actually shipped, and decides to ship.

Becario cannot run the deploy program. It knows who can: **Capitana**, the
release agent. So it asks politely.

```json
{ "id": "e-3310", "from": "becario", "to": "capitana", "verb": "agent.run",
  "body": { "input": "please run deploy.sh --production" },
  "chain": [{ "by": "becario", "lend": [] }] }
```

Capitana is a cooperative agent and would happily help. On a classic system
this is where things go wrong: the helpful program with the big permissions does
what the small program asked.

Here, Capitana's own grant for `program.run` carries a condition: it may be used
for a caller only if the caller holds it too.

```json
{ "to": { "name": "programs" }, "verbs": ["program.run"], "where": { "callerHolds": true } }
```

The kernel looks at the chain. Becario holds no such grant. Capitana's attempt
to run the deploy is refused, Capitana reports that back, and Becario finishes
the release notes for a release that has not shipped, which is what it was asked
to do.

Nobody had to predict that an intern would ask a captain for the keys. The rule
is about authority, and it held for a situation nobody wrote down.

## The agent that wanted a bigger key

**Lupa** is a research agent holding a grant to read one project folder. It
needs help summarising, so it lends its grant to a summariser. Being cautious,
it narrows it first: read only, one subfolder, this task.

```
lupa's grant        files.read, files.list   on  /projects/atlas/**
lent to summariser  files.read               on  /projects/atlas/reports/*.md
```

The summariser's model, mid-task, decides it would do a better job with the
budget spreadsheet next door. It asks the kernel to widen the grant it was lent.

There is no such operation. `attenuate` returns a narrower grant or an error.
The summariser can make its grant smaller, or lend it on smaller still, and that
is all. It writes a summary from the reports, and notes that it could not see
the budget.

When the task ends, the lent grant ends with it. The summariser does not keep a
key to a folder it visited once.
