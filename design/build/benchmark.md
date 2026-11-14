# Building the benchmark

[Index](../README.md) · [Concept](../benchmark.md) · [Stories](../stories/benchmark.md)

How to implement the coding benchmark with what the repository has today. Code
blocks are sketches; they have not been compiled.

## What exists to build on

| Today | Where | Use |
|---|---|---|
| A coder agent that writes code, runs it and fixes it | `examples/cli-harness/plugins/coder` | The scaffold |
| A workspace with an allowlist of programs, a timeout and a reduced environment | `host/src/workspace.rs` | Where a task runs |
| A model client that reaches OpenRouter (`openrouter/<id>`) and LM Studio (`local/<id>`) | `modules/llm-call` | Both kinds of model, already |
| `--model` on the host | `host/src/main.rs` | Switching the model per run |
| A smoke script | `examples/cli-harness/smoke.sh` | The pattern for a scripted run |

Missing: tasks, grading, per-run accounting of tokens, and anything that
aggregates results.

## Layout

```
bench/
  profiles.json            the dimensions and their allowed values
  models.json              model ids, where each runs, paid twin and its price
  seeds/<language>/<name>/ a seed repository: source, build files, visible tests
  tasks/<id>/
    task.json              profile, seed, statement, budgets
    setup.patch            turns the seed into the starting state
    hidden/                tests the agent never sees
    grade.sh               builds, runs visible and hidden tests, prints one JSON line
  runner/                  the program that executes runs
  results/runs.jsonl       one line per run (git-ignored)
  table/                   the fitted table and the selector
```

## A task

```json
{
  "id": "py-fixbug-pagination-01",
  "profile": { "language": "python", "task": "fix-bug", "libraries": "popular",
               "repo": "single-package", "env": "uv+pytest" },
  "seed": "seeds/python/orders-api",
  "statement": "Listing orders returns the last page twice when the total is a multiple of the page size. Fix it.",
  "budget": { "model_calls": 30, "seconds": 600 },
  "held_out": false
}
```

`grade.sh` is the only judge. It prints, for example:

```json
{ "build": true, "visible_passed": 14, "visible_total": 14, "hidden_passed": 3, "hidden_total": 3 }
```

A run is **solved** when the build succeeds, every hidden test passes, and no
visible test that passed before now fails.

## The runner

For one `(task, model, attempt)`:

1. Copy the seed to a fresh workspace folder and apply `setup.patch`.
2. Start the harness with that workspace and `--model`, and give the coder agent
   the statement. Apply the per-run variation to the statement (reworded
   sentence order, renamed example values) from a seed derived from the attempt.
3. Stop at the budget.
4. Copy `hidden/` in, run `grade.sh`, remove `hidden/` again.
5. Append one line to `runs.jsonl`.

```json
{ "task": "py-fixbug-pagination-01", "model": "openrouter/google/gemma-4-31b-it:free",
  "attempt": 2, "solved": true, "model_calls": 11, "tool_calls": 19,
  "tokens_in": 48210, "tokens_out": 3920, "seconds": 187,
  "tool_errors": 1, "malformed_replies": 0, "stopped_by": null,
  "diff_lines": 9, "scaffold": "coder@3bedb33", "date": "2026-11-20" }
```

### Changes needed in the host

| Change | Why |
|---|---|
| Return token counts from `llm-call` | The provider's reply has a `usage` block; today only the text is passed back |
| Count model and tool calls per run | For effort and for the budget |
| A budget of calls and seconds on a harness run | A weak model must not loop for an hour |
| Write the workspace path and model per run from the command line | So the runner can script it |

These are the same things the [ledger](../protocol.md#ledger) records. If the
ledger is built first, the runner reads it and none of this is special to the
benchmark.

## Languages and toolchains

One line per language decides what a seed repository uses and what the workspace
must allow.

| Language | Build and packages | Tests |
|---|---|---|
| Python | `uv` | `pytest` |
| TypeScript | `pnpm`, `tsc` | `vitest` |
| Rust | `cargo` | `cargo test` |
| Java | `gradle` | JUnit |
| JavaScript | `pnpm` | `vitest` |
| C | `cmake` | CTest |
| C++ | `cmake` | Catch2 through CTest |
| C# | `dotnet` | `dotnet test` |
| Go | `go` | `go test` |
| SQL | A database in a container, migrations as files | Queries compared with expected rows |

Linux runs use one container image per language, so toolchain versions are
pinned and recorded. macOS runs use the machine's toolchains, with versions
written into each result.

## Models

`models.json` records how to reach each model and what its paid twin costs:

```json
{ "id": "openrouter/nvidia/nemotron-3.5-lightning:free", "runs": "cloud",
  "twin": "nvidia/nemotron-3.5-lightning", "usd_per_mtok": [0.07, 0.20],
  "local_twin": "local/nvidia-nemotron-3.5-lightning-30b-a3b" }
```

Prices are read from OpenRouter's model list when a batch starts and stored with
the results, because they change.

**Free endpoints.** They are rate limited. The runner needs a queue per
provider, a pause when it is told to slow down, and a daily plan that fits the
limit. Check the current limit before planning; it has been in the order of
tens to a thousand requests a day depending on the account.

**Local models.** LM Studio loads one large model at a time on a 32 GB machine.
Group runs by model, so each model is loaded once per batch.

**Hybrid.** A configuration, not a model: the local model first, and the cloud
model when the local one exhausts half the budget without passing the visible
tests. It is run and scored like any other entry.

## Qualifying round

Before the pilot, to pick the five cloud models and confirm the five local ones
from the candidates:

- Six tasks, two per tier 1 language, one attempt each.
- Drop a model that fails to produce well-formed tool calls on most runs, or
  that solves nothing.
- Cost: with eight cloud candidates and six local ones, 84 runs.

## Size of the pilot

| | Count |
|---|---|
| Tasks | 30 (10 per tier 1 language), 10 of them held out |
| Models | 10 |
| Attempts | 3 |
| Runs | 900 |

Rough time, to be replaced by measurement from the qualifying round: a local run
of a 30B model may take five minutes, so the 450 local runs are about a day and
a half of machine time. The 450 cloud runs are bounded by the rate limit, not by
speed.

## The table

1. **Pair coverage.** Generate the list of `(task, model)` runs with a standard
   pairwise generator over the profile dimensions, then add repeats.
2. **Fit.** A logistic model of `solved` with a term per dimension value, a term
   per model, and model-by-language and model-by-task interactions, with
   partial pooling so that sparse cells borrow from similar ones. Start with
   the simplest version that fits: per-cell rates with intervals, and only
   estimate cells that have no runs.
3. **Write** `table/table.json` in the shape shown in the
   [concept](../benchmark.md#from-runs-to-the-table), with `basis` set to
   `measured` or `estimated` and wide intervals reported as unknown.
4. **Trade-off frontier.** For each profile, keep the models that no other model
   beats on both success and shadow price.

## Profiler and selector

```
profile(repo):
  language   <- the language with most source lines, ignoring vendored folders
  env        <- from lock and build files: uv.lock, pnpm-lock.yaml, Cargo.toml, CMakeLists.txt ...
  repo       <- file count, workspace or monorepo markers, presence of CI configuration
  libraries  <- dependencies compared with a list of popular ones per language
  os         <- where it is run

select(profile, need):
  rows <- table rows for the profile, or the nearest profile with measured cells
  return rows on the trade-off frontier, ordered by `need` (quality, price, or speed)
```

The selector says when it fell back to a nearer profile, and when the best
answer is "unknown".

## Order

1. `task.json`, one Python seed repository, three tasks, `grade.sh`.
2. The runner, with solved and seconds only.
3. Token counts and call counts.
4. Tier 1 seeds and thirty tasks.
5. Qualifying round.
6. Pilot.
7. Per-cell table and the trade-off frontier.
8. Profiler and selector.
9. Tier 2 and 3, pair coverage, the fitted model.

## How to know it works

| Check | Kind |
|---|---|
| A run with the reference solution applied is graded solved; the untouched starting state is graded unsolved | Test for every task, run in CI |
| The agent's workspace never contains `hidden/` while the agent runs | Runner test |
| A change that deletes or skips a visible test is graded unsolved | Grading test |
| The same run twice gives the same grade | Determinism test of `grade.sh` |
| Token counts match the provider's usage for a sample of runs | Spot check |
| The profiler returns the right profile for each seed repository | Unit tests |
| The selector returns "unknown" for a profile with no nearby measured cells | Unit test |
