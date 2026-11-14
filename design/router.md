# Router: sending each piece of work to the right expert

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [Benchmark](benchmark.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/router.md) · [stories](stories/router.md)

Status: plan, with one small experiment behind it
([below](#what-a-first-experiment-showed)). Nothing here is built.

## The idea

The [benchmark](benchmark.md) produces a table: for a kind of project and a kind
of task, which model and agent do best. The router is what uses that table while
work is happening. It looks at each piece of development work, decides what it
is, and sends it to the expert that the table says is best for it.

The deciding is done by **decision models**: small models that do not write
text. They answer typed questions about a message (pick one of these, yes or no,
how much, extract this) with a probability for each answer. They are fast, they
run locally, and they say how sure they are.

## What gets decided, and by what

Not everything about a piece of work needs a model. The router uses the cheapest
source that is reliable for each fact.

| Fact | Source | Why |
|---|---|---|
| Language, toolchain, libraries, repository shape | **Profiler**: reads the repository | These are facts on disk. A model guessing them from a sentence does worse than reading `Cargo.toml` |
| Kind of task: bug, feature, refactor, tests, upgrade, build | **Decision model** | It is in the wording of the request |
| Phase: plan, implement, test, debug, review | **Decision model** | Same |
| Does it need to run programs, touch the network, change many files | **Decision model**, yes or no | Decides which grants the expert will need |
| Is it risky: production, secrets, deleting data | **Decision model**, then a guard | A risky piece goes to a person or a stricter expert |
| What matters most: quality, price or speed | Policy and the remaining budget | Not a property of the request |

Together these make the same **profile** the benchmark table is indexed by.

## Tiers

```
request ─▶ profiler ─▶ tiny decision model ──sure──▶ table ─▶ expert pool
                              │
                           not sure
                              ▼
                     larger decision model ──sure──▶ table ─▶ expert pool
                              │
                           not sure
                              ▼
                     generalist agent, or a person
```

1. **Profiler.** No model. Always runs.
2. **Tiny decision model.** A few megabytes, on the local machine. Its answer is
   used only above a confidence threshold.
3. **Larger decision model.** A few billion parameters, still local. Asked only
   when the tiny one is not sure.
4. **Fallback.** The best generalist for the profile the profiler found. If the
   piece was marked risky, a person.

The rule that makes the tiers safe: **a decision below its threshold is not a
decision.** The router never acts on a guess it was told was a guess.

## From decision to expert

With a profile, the router looks up the knowledge table and gets a ranking with
its trade-off frontier. It picks along the frontier according to what matters
most for this intent, and sends the work to that expert's pool.

In protocol terms the router is how a pool with `dispatch: any` chooses its
member. It adds nothing new to the protocol: the request is an intent, the
router's choice is an intent to a pool, and both are in the ledger.

**An expert can decline.** If the work turns out not to be what the router
thought, the expert answers `declined` with a reason instead of doing it badly.
The router then tries the next on the ranking, once, within the same budget. A
wrong decision costs one extra hop instead of a wrong result.

## Parts of the work

"Detecting parts of development" can mean two sizes of part.

| Unit | Example | When |
|---|---|---|
| A whole task | "Fix the pagination bug" goes to one expert | First. One decision per task |
| A step inside a task | Planning to one model, writing code to another, reviewing to a third | Later. The agent that drives the task emits each step as its own intent, and the router decides each |

Routing by step can use a cheap model for most steps and an expensive one only
where it matters. It also multiplies the decisions, and with them the chances of
a wrong one. It should be turned on only for the kinds of step where the
benchmark shows the experts really differ.

## Learning from what happens

Every routed intent leaves three things in the ledger: what the router decided
and how sure it was, which expert got the work, and how it ended.

- **The table stays current.** Outcomes of real work update the same cells the
  benchmark filled.
- **A share of work explores.** A small fraction goes to an expert that is not
  the top of the ranking, so the table notices when a ranking has changed. This
  is a [canary](hot-swap.md) in all but name.
- **The router is improved like any agent.** Its thresholds and its questions
  are settings, so the [evolution loop](evolution.md) can propose better ones,
  judged on routing accuracy.
- **Corrections.** Some decision models can take corrected examples and adjust
  without retraining. Worth using where it helps; the first test found it did
  not ([below](#what-a-first-experiment-showed)).

## What a first experiment showed

One tiny decision model, [TinyDecide](https://huggingface.co/TheREZOR/TinyDecide)
(10.4M parameters, 6 MB, Apache-2.0), was run locally on 32 hand-written
development requests, each labelled with its language and its kind of task.

| Question | Correct | When confident (0.5 or more) |
|---|---|---|
| Kind of task, 6 options | 69% | 9 of 9 correct; 9 of 32 requests reached that confidence |
| Language, 10 options | 34% | Never reached that confidence |

- **Kind of task works behind a threshold.** When the model was sure, it was
  right, and when it was not, it said so.
- **Language does not work.** The model was trained on everyday messages. It
  took `pytest` for Go and `cargo` for Python, at confidences near 0.01. This is
  the evidence for the profiler: read the language from the repository.
- **Corrections did not help.** With 36 separate examples, language stayed at
  34% and kind of task went from 69% to 66%. The model's own check chose to
  ignore the language corrections.
- **Speed.** About 600 to 800 ms per request with the JavaScript engine on an
  Apple M5. The Rust engine was not tried.

The sample is small and written by one author, so these are indications. They
are enough to set the design: a tiny model is a useful first tier for some
questions and useless for others, and its confidence tells the two apart.

Not yet tested: the larger decision models, the Rust engine, and any of this on
real requests.

## Limits

- **Wrong decisions compound.** A misrouted task can end worse than if the best
  generalist had done everything. Thresholds, declining and the fallback bound
  this; they do not remove it.
- **The router adds time** to every piece of work. It is worth it only when the
  experts differ enough, which is what the benchmark is for.
- **A cold table routes badly.** Until cells are measured, the router should
  fall back often.
- **Decision models read English.** The tiny one tested reads only English and
  only the first 127 tokens.
- **Text is not the whole request.** Two identical sentences can be different
  work in different repositories. The profiler covers part of this.

## Sources

- [TinyDecide model card](https://huggingface.co/TheREZOR/TinyDecide), for the model, its question types, its corrections and its stated limits.
- [Decision Index](https://huggingface.co/spaces/multimodalart/jev-decision-index), the public board for typed decision models that the card refers to.
