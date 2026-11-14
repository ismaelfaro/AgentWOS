# Benchmark: choosing the model for a coding agent

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

See also: [how to build it](build/benchmark.md) · [stories](stories/benchmark.md)

Status: plan. Nothing here is built. Model and language lists were checked on
14 November 2026 and will age quickly.

## What it is for

A coding agent is a scaffold plus a model. Which model is best depends on the
project: its language, its tools, where the model may run. This benchmark
measures current models on programming tasks and turns the results into a
**knowledge table**: given the profile of a project, which model to give its
agent, at what cost, with how much confidence.

In AgentWOS terms it is the [Evaluator](evolution.md) run at scale, and its
table is what a pool of model agents uses to route `model.complete`.

## The profile of a project

| Dimension | Values in the first version |
|---|---|
| Language | Ten, in three tiers (below) |
| Operating system | Linux, macOS. Windows later |
| Development environment | Package manager, build tool, test runner of each language |
| Repository | Size, single package or monorepo, CI present, git history |
| Libraries | Standard library only, popular libraries, obscure or recent libraries |
| Where the model runs | Local, cloud, hybrid (local first, cloud when stuck) |
| Kind of task | Fix a bug, add a feature, refactor, write tests, upgrade a dependency, repair a build or CI |

The full grid of these is far too large to run. The plan is to measure a chosen
sample and estimate the rest ([below](#from-runs-to-the-table)).

## Languages

The seven requested, plus three to reach ten, chosen from industry rankings.

| Tier | Languages | Why |
|---|---|---|
| 1, pilot | Python, TypeScript, Rust | Requested. First on TIOBE; first by contributors on GitHub; the language of this repository |
| 2 | Java, JavaScript, C, C++ | Requested. Second to fourth on TIOBE, and first by usage in the Stack Overflow survey |
| 3 | C#, Go, SQL | Added. Fifth, tenth and eighth on TIOBE; SQL is second by usage in the Stack Overflow survey |

Notes on the choice:

- Rust is 11th on TIOBE this month, just outside the top ten. It stays because
  it was requested and because the system under study is written in it.
- R and Visual Basic are in TIOBE's top ten and are left out: little of their
  use is the kind of repository work this benchmark measures.
- SQL tasks have a different shape (queries and migrations against a database),
  so tier 3 needs its own task templates.
- Rankings disagree because they measure different things: TIOBE counts search
  interest, Stack Overflow asks what developers used, GitHub counts
  contributors.

## Models

Ten to start: five free on OpenRouter and five that run on the local machine
(an Apple M5 with 32 GB, so about 30B parameters, one model loaded at a time).

### Cloud, free on OpenRouter

Taken from OpenRouter's model list, which had 14 free text models with tool
support on the day of writing.

| Model id | Why this one | List price of the paid twin, per million tokens (in / out) |
|---|---|---|
| `cohere/north-mini-code:free` | Described by its vendor as a coding model | None listed |
| `nvidia/nemotron-3-ultra-550b-a55b:free` | Largest free model; already the default in the CLI harness | $0.60 / $2.40 |
| `google/gemma-4-31b-it:free` | Open weights; a smaller sibling runs locally | $0.20 / $0.40 |
| `thinkingmachines/inkling-2-small:free` | Open-weight reasoning model | $0.30 / $1.00 |
| `nvidia/nemotron-3.5-lightning:free` | The same model is installed locally, so local and cloud can be compared directly | $0.07 / $0.20 |

Reserves, if one of those is withdrawn: `baidu/ernie-5.5-flash-preview:free`,
`inclusionai/ling-3.2-flash:free`, `nvidia/nemotron-3.5-super-vl:free`.

This selection is by description and size, not by known quality. Most of these
models are recent and no independent coding results were found for them. A short
qualifying round decides the final five
([build](build/benchmark.md#qualifying-round)).

### Local, already installed in LM Studio

| Model id | Why this one |
|---|---|
| `qwen/qwen3.6-35b-a3b` | The Qwen 3.6 family has the strongest published coding claims among models this size |
| `nvidia-nemotron-3.5-lightning-30b-a3b` | Twin of the cloud model above |
| `muse-glimmer-30b` | Listed in guides as fitting a 24 GB card, with a published SWE-bench Pro score |
| `ornith-1.5-35b-a3b-apex-mtp` | A second 35B mixture-of-experts model, for contrast |
| `google/gemma-4-e4b` | Small baseline; the model the Mars example already uses |

Worth installing later: a Qwen coder model of about 30B, which several guides
name as the most recommended local coding model.

### Frontier models

The request is to compare frontier models with open ones, on a small budget. The
free models above are open or preview models, not the frontier. To have the
comparison without the cost:

- Run paid frontier models on a **fixed subset** of tasks (one in five), once.
- Set a spending cap before starting, and stop at it.
- Treat them as an anchor: "the best free model reaches this fraction of the
  frontier score on this profile, at this fraction of the price".

Which frontier models, and the cap, are decisions for the owner of the budget.

## Value for money

A free model costs nothing today, so "price" needs a definition that still means
something next month.

| Measure | How |
|---|---|
| Tokens per solved task | Counted per run |
| **Shadow price** | Tokens priced at the paid twin's list price, when there is one |
| Time per solved task | Wall clock, per run |
| Local cost | Time on the machine; optionally energy |
| Score per dollar | Success rate divided by shadow price per task |

The table reports quality and price as separate columns and one combined view: a
**frontier of the best trade-offs**, the models for which no other is both
better and cheaper on that profile.

## Tasks

All tasks are written for this benchmark, so no model has seen them in training.

- **Seed repositories.** A few small, realistic projects per language, each with
  a working build and a test suite.
- **Templates.** A task is a seed repository, a change that breaks or extends
  it, a statement of what to do, and hidden tests that tell success from
  failure. One template yields several tasks by varying one dimension: the
  library used, the size of the repository, the kind of task.
- **A held-out third.** One task in three is never used to tune an agent or a
  prompt. It is only measured.

One risk is specific to free endpoints: they may log prompts, and logged prompts
can become training data. Three measures: hidden tests never leave the machine,
since the agent never sees them; task statements carry a per-run variation; and
the held-out third is run on free endpoints last and replaced after each public
release of results.

## Running and grading

- **Fixed scaffold, variable model.** The coder agent of the CLI harness and its
  workspace are the scaffold. Only the model agent changes. The pilot runs a
  second scaffold on a subset, to measure how much the scaffold matters.
- **Graded by execution.** Hidden tests must pass, existing tests must still
  pass, and the build must succeed. No model judges another model's work.
- **Repeated.** Each task runs three times per model. A model that solves a task
  once in three is not the same as one that solves it every time.
- **Bounded.** Each run has a budget of calls and seconds.

## What is measured

| Measure | Tells |
|---|---|
| Solved, of three attempts | Quality and reliability |
| Tokens, shadow price, time | Cost |
| Model calls and tool calls | Effort |
| Tool errors, malformed replies, budget exhausted | How it failed |
| Size of the change | Whether it did only what was asked |

## From runs to the table

1. **Cover pairs, not the grid.** Choose runs so that every pair of values from
   two dimensions appears at least once. That is a small fraction of the full
   grid.
2. **Fit a model of the results.** Success as a function of each dimension, plus
   how each model interacts with each dimension.
3. **Fill the table.** For each profile and model: an estimated success rate
   with an interval, cost, time, and whether the cell was measured or estimated.
4. **Say when not to trust it.** A cell with a wide interval is reported as
   unknown.

```json
{
  "profile": { "language": "rust", "os": "macos", "libraries": "popular", "runs": "local", "task": "fix-bug" },
  "ranking": [
    { "model": "qwen/qwen3.6-35b-a3b", "solved": 0.61, "interval": [0.48, 0.73],
      "shadow_usd_per_task": 0.0, "seconds_per_task": 410, "basis": "measured" },
    { "model": "nvidia-nemotron-3.5-lightning-30b-a3b", "solved": 0.44, "interval": [0.29, 0.60],
      "shadow_usd_per_task": 0.0, "seconds_per_task": 190, "basis": "estimated" }
  ],
  "measured_on": "2026-11-20", "tasks": 24
}
```

The numbers in that example are invented to show the shape.

## Using the table

A **profiler** reads a repository (languages by file count, build and lock
files, CI configuration, size) and produces its profile. A **selector** looks
the profile up and returns the ranking, with the trade-off frontier marked.

In AgentWOS the selector is what a pool of model agents consults, and the table
is kept current by the [evolution loop](evolution.md): when the Scout finds a
new model, the benchmark is how the Evaluator judges it.

## Phases

1. Profile schema; three seed repositories and thirty tasks in the tier 1
   languages.
2. Runner, grading and the record of each run.
3. Qualifying round, then the pilot: ten models, tier 1, three attempts.
4. Tier 2 and tier 3 languages, with pair coverage.
5. The fitted model, the table, the profiler and the selector.
6. Frontier anchor, within the cap.
7. Continuous refresh as models appear and disappear.

## Limits

- **Scaffold and model are entangled.** A model may do better with a different
  prompt. The second scaffold measures the size of this, and does not remove it.
- **Free endpoints change.** They have low rate limits, and preview models are
  withdrawn. Each result records the model id and the date.
- **Local runs are slow.** A 30B model on this machine takes tens of seconds per
  call. The pilot is a matter of days, not hours.
- **Windows is missing** from the first version.
- **Estimated cells are estimates.** The table says which they are.
- **A benchmark that is optimised against stops measuring.** That is the reason
  for the held-out third.

## Sources

- [TIOBE Index](https://www.tiobe.com/tiobe-index/) for November 2026, and [TechRepublic's summary](https://www.techrepublic.com/article/news-tiobe-november-2026-java-overtakes-cpp/).
- Stack Overflow, [2026 Developer Survey: technology](https://survey.stackoverflow.co/2026/technology) and the [announcement](https://stackoverflow.blog/2026/10/06/the-results-of-the-2026-developer-survey-are-here/).
- [What the language rankings measure](https://codearchaeology.dev/blog/top-programming-languages-2026/), on why TIOBE, Stack Overflow and GitHub differ.
- OpenRouter model list, read from its public API on 14 November 2026.
- Guides to local coding models: [Morph](https://www.morphllm.com/best-ollama-models), [Kilo](https://kilo.ai/open-source-models), [Digital Applied](https://www.digitalapplied.com/blog/best-open-weight-coding-models-self-host-hardware-match-2026). These are secondary sources whose scores disagree with each other.
