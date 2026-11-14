# Stories: the router

[Index](../README.md) · [Concept](../router.md) · [Build](../build/router.md)

Invented scenarios that show the profiler, the decision tiers, declining and
learning from outcomes. In the first story, the wrong language answer at 0.01
and the totals over thirty-two requests are what really happened when a tiny
decision model was tried. Every other figure and every other story is made up.
The mechanics are the ones in the plan.

## The receptionist

At the front desk of the building sits the smallest employee in the company. Six
megabytes. It cannot write code, hold a conversation or explain itself. It
answers multiple-choice questions about whatever lands on the desk, and next to
every answer it writes how sure it is.

A request arrives: *"pytest fails with an import error in test_invoice.py after
the last change, find out why and fix it."*

> What kind of work is this? **Fix a bug.** Sure: 0.71.
> Which programming language? **Go.** Sure: 0.01.

The first answer is right. The second is spectacularly wrong, and the
receptionist knows it: one in a hundred is its way of saying *I have no idea,
please do not listen to me*.

Nobody does. The rule in this building is that an answer below its threshold was
never given. And the language was never the receptionist's question to begin
with: down the hall, the **profiler** has already opened the repository, found
`pyproject.toml` and four hundred `.py` files, and written "Python" on the form
without consulting anyone.

```json
{ "profile": { "language": "python", "env": "uv+pytest", "task": "fix-bug" },
  "decided_by": { "language": "profiler", "kind": "decide-tiny" } }
```

The form goes to the table, the table names the best expert for Python bug
fixes, and the work is on its way in under a second.

When this receptionist was actually tried on thirty-two requests, it was right
about the kind of work two times in three, and right nine times out of nine when
it said it was sure. On the language it was right one time in three, and never
once claimed to be sure. A colleague who is often wrong and always tells you is
more useful than it sounds.

| What happened | Part of the plan |
|---|---|
| Language came from the repository | Profiler; facts on disk are not guessed |
| "Fix a bug" at 0.71 was used | Above the threshold |
| "Go" at 0.01 was ignored | Below the threshold, so not a decision |

## Ask upstairs

The next request is less tidy: *"the numbers on the dashboard look off since
Tuesday."*

The receptionist reads it twice. Bug? Feature? Build? Its best answer is "fix a
bug" at 0.31. Not sure. It does what it is paid to do with an unsure answer,
which is nothing.

The request goes upstairs to a larger decision model, two billion parameters and
a slower lift. It considers "since Tuesday" and "look off" and says: fix a bug,
0.78; needs to run programs, 0.91; could affect a production system, 0.62.

That last answer changes the route. Above the risk threshold, the work does not
go to an expert at all. It goes to Nuria, with the three answers attached, and a
question: *this looks like a bug in something live. Who should look?*

Three tiers looked at one sentence. The cheapest one took 600 milliseconds and
declined to answer. That was the correct outcome, and it cost almost nothing.

## Not my department

The router sends a task to the Python expert: *"the monthly report double counts
orders with two shipments."* The repository is a Python service, so the profiler
said Python, and the receptionist said bug.

The Python expert opens the code and finds that the report is one line of Python
calling four hundred lines of SQL, and the bug is in a join.

On a system without manners, the Python expert would now have a go at the SQL,
because it was asked. Here it answers:

```json
{ "declined": "the fault is in reports/monthly.sql; this needs the SQL expert" }
```

The router takes the decline as news about its own decision. It tries the next
expert on the ranking, once, and the SQL expert fixes the join. In the ledger the
detour is charged to the router, with the reason, so that the next time a
request mentions a report in this repository, the profile it builds starts from
the file that matters.

One wrong turn cost one extra hop. It did not cost a confident Python programmer
rewriting a query it did not understand.

## The two-euro lesson

Lucas's project is a TypeScript front end. The table says model A is best for
it, and model A is not cheap. Lucas has a budget, so the policy for his project
says price matters most.

The router does not send everything to the cheapest model. It routes by step.

- *Rename this prop everywhere*: refactor, low risk. The cheap local model.
- *Why does the cart re-render on every keystroke*: debugging. The table says
  the experts differ a lot here. Model A.
- *Write tests for the price formatter*: tests. The cheap local model.
- *Review the diff before I push*: review. Model A.

At the end of the week the ledger adds it up: 212 steps, 31 of them on the
expensive model, two euros and ten cents. Sending all 212 to model A would have
cost eleven. Sending all of them to the local model would have cost nothing and
left the re-render bug where it was, which the ledger also knows, because for
five intents in a hundred the router deliberately tried the other choice and
wrote down what happened.

## The router gets a review

After a month the Observer reads the router's own entries.

The receptionist's threshold for "kind of work" is 0.5. In a thousand routed
requests it answered 290 and was right on 281. The Observer notices something
else: between 0.4 and 0.5 it was right on 88 of 96.

The Proposer suggests lowering the threshold to 0.4. The Evaluator replays the
month: a third more requests settled at the front desk, about one more wrong
route in a hundred, each of which costs one decline. Faster on average.

The router is a system agent, so the change is in `propose` mode. It goes to
Nuria with the numbers. She approves 0.45, because she is like that.

Nobody retrained anything. The smallest employee in the building got a slightly
longer list of things it is allowed to be sure about.
