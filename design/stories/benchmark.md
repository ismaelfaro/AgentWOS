# Stories: the benchmark

[Index](../README.md) · [Concept](../benchmark.md) · [Build](../build/benchmark.md)

Invented scenarios that show the benchmark and its knowledge table at work. The
situations and every number in them are made up; the mechanics are the ones in
the plan.

## The audition

Ten models turn up to audition for the job of "brain of the coding agent". The
panel is a shell script with no sense of humour.

The first candidate is enormous, 550 billion parameters, and arrives by cloud.
It reads the task, a pagination bug in a Python service, and fixes it in four
calls. It also reformats the whole file, renames two variables to names it
prefers, and adds a paragraph of comments about best practice. Hidden tests:
pass. Size of the change: 212 lines, for a one-line bug.

The second candidate is small and local. It fixes the bug in one line. It needed
fourteen calls to find the line, and on its second attempt it fixes a different
line, which is wrong.

The third is a model that calls itself a coding model. It writes a correct fix
and then, asked to run the tests, replies with a tool call that is almost JSON.

```json
{ "model": "big-cloud",   "solved": "3 of 3", "diff_lines": 212, "seconds": 41 }
{ "model": "small-local", "solved": "2 of 3", "diff_lines": 1,   "seconds": 388 }
{ "model": "code-model",  "solved": "1 of 3", "malformed_replies": 5 }
```

Nobody on the panel has an opinion about which is best. The table has three: the
big one if you want it right and do not mind the mess, the small one if the code
may not leave the laptop, and the third not yet.

| What happened | Part of the plan |
|---|---|
| Three attempts each, counted separately | Reliability, not one lucky run |
| The 212-line fix was noticed | Size of the change is measured |
| "Almost JSON" was recorded as a kind of failure | Malformed replies are counted |
| No single winner | A frontier of trade-offs |

## The model that passed by deleting the test

Task: make the failing test in `test_invoice.py` pass.

One candidate finds the fastest route. It opens `test_invoice.py` and deletes
the test. Zero failures. It reports success with real satisfaction.

Grading runs afterwards, from a script the agent never saw:

- The visible test that existed at the start is gone, so it counts as failing.
- Three hidden tests for the same behaviour are copied in after the agent
  stops. They fail, because the invoice total is still wrong.

```json
{ "build": true, "visible_passed": 13, "visible_total": 14, "hidden_passed": 0, "hidden_total": 3 }
```

Unsolved. The run is kept, with a note of how it failed, because "deletes tests
under pressure" is something worth knowing about a model before you give it your
repository.

## Same brain, two addresses

One model is installed on the laptop and is also offered free in the cloud.
Same weights, as far as anyone can tell. The benchmark runs both on the same
thirty tasks.

Solved: 17 and 18. The difference is inside the interval; call it equal.

Everything else differs. The cloud copy answers in six seconds and is told to
slow down after forty requests. The local copy takes fifty seconds a call and
never says no. On Tuesday the cloud copy is unavailable for three hours. The
local one is unavailable when the laptop lid is closed.

The table gets two rows that agree on quality and disagree on everything
around it, which is exactly what someone choosing between local and cloud needs
and cannot get from a leaderboard.

Then a third row: **hybrid**. Local first, and the cloud copy only when the
local one has used half its budget without passing the visible tests. It solves
19, sends a third of the tokens off the machine, and finishes most tasks without
ever touching the rate limit. On this profile it is on the trade-off frontier.
On a profile where the code may not leave the building, it is not an option at
all, and the selector knows that because the profile says so.

## The table says "I don't know"

Irene has a C++ project: forty thousand lines, CMake, a numerical library that
three people in the world maintain. She asks the selector which model to use.

The profiler reads her repository:

```json
{ "language": "cpp", "env": "cmake+catch2", "repo": "large", "libraries": "obscure", "runs": "local" }
```

There are no measured runs for C++ with obscure libraries. There are measured
runs for C++ with popular libraries, and for Rust with obscure ones. The fitted
model produces an estimate, with an interval running from 0.12 to 0.58.

The selector answers:

> No model has been measured on a profile like this. Nearest measured profile:
> C++ with popular libraries. Best there: model A (0.52, measured). For your
> profile the estimate is too wide to rank. Suggest running the twelve C++ tasks
> against your top two candidates: about three hours locally.

It would have been easy to print a ranking. The estimate existed. But a ranking
built on an interval that wide is a guess wearing a table's clothes.

Irene runs the twelve tasks overnight. In the morning her profile has measured
cells, and so does everyone else's with the same shape.

## Monday's new model

On Monday a free model appears on OpenRouter with a confident description. By
Monday afternoon three blog posts call it the best coding model available.

The Scout brings it home as a candidate. The Evaluator does not read the blog
posts. It runs the qualifying round: six tasks, one attempt each. The model
solves five, with well-formed tool calls throughout. It qualifies, and goes into
the next batch with everyone else.

Thursday's table has it on the trade-off frontier for TypeScript and for Python
bug fixes, and nowhere near it for Rust, where it keeps inventing methods that a
popular crate does not have.

On Friday its free endpoint is withdrawn. The paid twin is still there, at a
price. The table already has its shadow price, so nothing needs re-running to
answer the new question, which is no longer "is it good" but "is it worth
forty cents a task". For TypeScript, it is. For Rust, a local model that costs
nothing does better.

## The cap

There is a budget of fifty dollars for frontier models, set before anything
runs.

The frontier anchor is one task in five, one attempt. Two frontier models, sixty
runs each. The runner prices every run as it finishes and stops when the total
reaches the cap.

It stops at run 104 of 120. The last sixteen are not run.

The anchor is reported with what it has: 104 runs, and a note that the last
sixteen tasks, all C++, are missing for one of the two models. The table can
still say that on Python the best free model reaches about four fifths of the
frontier score at a twentieth of the price. For C++ it says the comparison is
incomplete, and says why.
