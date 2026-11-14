# Building the router

[Index](../README.md) · [Concept](../router.md) · [Stories](../stories/router.md)

How to implement the profiler, decision agents, the router and its measurement.
It assumes the benchmark's knowledge table ([Building the benchmark](benchmark.md))
and, for the full version, pools and the ledger
([Building the protocol](protocol.md)). Code blocks are sketches; they have not
been compiled.

## Components

| Component | Kind | Serves |
|---|---|---|
| Profiler | A WASM agent with a read-only files grant on the repository | `profile.read` |
| Decision agent, tiny | A WASM agent wrapping a decision model engine | `decide.answer` |
| Decision agent, larger | A model agent reached through LM Studio | `decide.answer` |
| Router | A system agent | `route.request` |
| Knowledge table | A file, read by the router | (data) |

## Profiler

Deterministic, no model. The same function the benchmark's selector uses.

```
profile.read(repo) ->
  language    most source lines by extension, ignoring vendored and generated folders
  env         from Cargo.toml, pyproject.toml / uv.lock, package.json / pnpm-lock.yaml,
              CMakeLists.txt, build.gradle, *.csproj, go.mod
  libraries   dependencies, compared with a list of popular ones per language
  repo        file count; workspace or monorepo markers; CI configuration present
  os          from the kernel
```

Cache the result per repository and invalidate it when a build or lock file
changes. A repository with several languages returns the profile of the files
the request names, when it names any.

## The decision interface

One verb for every decision model, whatever its size:

```json
{ "verb": "decide.answer",
  "body": {
    "state": "pytest fails with an import error in test_invoice.py, find out why and fix it",
    "questions": [
      { "id": "kind", "type": "choice", "text": "What kind of programming work is this?",
        "options": ["fix a bug", "add a feature", "refactor code", "write tests", "upgrade a dependency", "repair the build"] },
      { "id": "runs", "type": "yesno", "text": "Doing this requires running programs or tests." },
      { "id": "risky", "type": "yesno", "text": "This could delete data or affect a production system." }
    ] } }
```

```json
{ "answers": [
    { "id": "kind", "pick": "fix a bug", "confidence": 0.71, "probs": [0.71, 0.05, 0.03, 0.12, 0.02, 0.07] },
    { "id": "runs", "p": 0.88 },
    { "id": "risky", "p": 0.04 } ],
  "ms": 640, "model": "tinydecide-10.4m" }
```

The shape follows the tiny model tested (choice, yes or no, score, span). A
larger model that generates text is wrapped to the same shape: ask for JSON with
a probability per option, and treat a reply that does not parse as "not sure".

## The tiny decision agent

The model tested ships a Rust engine with four ordinary dependencies and no
system calls, so it should compile for `wasm32-wasip2` and run as an agent like
any other. That has not been tried.

1. A crate that depends on the engine and exports `handler`.
2. The weights (6 MB) and metadata (190 KB) included in the component with
   `include_bytes!`, so the agent needs no grant at all.
3. `handle("decide.answer", body)` maps the questions to the engine's types and
   returns the answers.

If the engine does not build for WASM, the fallback is the JavaScript engine in
the browser kernel and a native call in the host, with the same verb.

The first experiment measured 600 to 800 ms per request in JavaScript. Measure
the Rust build before deciding where the tiny tier sits: at that speed it is a
real cost on every request.

## The router

```
route.request(intent):
  profile   = profile.read(repo of the intent)
  questions = the router's settings
  for tier in settings.tiers:                       # tiny, then larger
      answers = ask tier decide.answer(intent text, questions)
      take every answer at or above settings.threshold[tier][question]
      if all needed answers are taken: break
  if a needed answer is still missing: profile.task = "unknown"
  if risky >= settings.risk_threshold: return escalate(person)

  ranking   = table.lookup(profile)                 # nearest measured profile, or unknown
  need      = quality | price | speed, from the policy and the remaining budget
  expert    = pick along the trade-off frontier for `need`,
              or the explore choice for a share settings.explore of intents
  result    = ask expert pool agent.run(intent)
  if result is declined and not yet retried: ask the next on the ranking
  return result
```

Settings, all of them changeable by the evolution loop in `propose` mode:

```json
{ "tiers": ["decide-tiny", "decide-2b"],
  "threshold": { "decide-tiny": { "kind": 0.5, "runs": 0.8, "risky": 0.9 },
                 "decide-2b":   { "kind": 0.6, "runs": 0.8, "risky": 0.8 } },
  "risk_threshold": 0.5, "explore": 0.05, "retries_after_decline": 1 }
```

The thresholds start from the experiment (0.5 on kind of task gave 9 of 9) and
are re-set from the router track below, not by feel.

## Declining

Add one outcome to the expert's reply: `{ "declined": "this is a SQL migration,
I work on Python" }`. The coder agents get a line in their instructions saying
when to decline, and the router treats a decline as information about its own
decision: it is recorded against the router, not against the expert.

## What goes in the ledger

```json
{ "intent": "e-5102", "verb": "route.request",
  "profile": { "language": "python", "env": "uv+pytest", "task": "fix-bug" },
  "decided_by": { "kind": "decide-tiny", "confidence": 0.71 },
  "ranking": ["expert-a", "expert-b"], "chosen": "expert-a", "explore": false,
  "outcome": "solved", "declined": 0, "router_ms": 655 }
```

`outcome` is filled when the work ends. For real work "solved" has no hidden
tests, so use what is available and say which: the project's own tests passing,
a person accepting the change, or no signal.

## Measuring routers: the router track

The benchmark's tasks already carry their profile, so they are labelled data for
the router.

```
bench/router/
  cases.jsonl      statement, profile, and 3 rewordings of each statement
  run.mjs          runs every decision agent on every case
  results.jsonl    per case and model: answers, confidences, ms
```

Rewordings matter: the task statements were written by the benchmark's authors,
and a router has to work on how other people phrase things. Write them by hand
for the pilot.

| Measure | Meaning |
|---|---|
| Accuracy | Correct answers over all cases, per question |
| Coverage at a threshold | Share of cases answered at or above it |
| Accuracy when confident | Correct answers among those |
| Calibration | Whether a confidence of 0.7 is right about 70% of the time |
| Milliseconds per request | Cost of the tier |
| **End-to-end gain** | Tasks solved with routing, against the best single model with no routing, at equal budget |

The last line is the one that decides whether a router is worth having. A router
with perfect accuracy is still useless if the experts do not differ.

## Order

1. Profiler, with unit tests on the benchmark's seed repositories.
2. `bench/router/cases.jsonl` from the pilot tasks, with rewordings.
3. Run the tiny model and the two larger local decision models on it.
4. Set thresholds from those results.
5. Router agent with whole-task routing, fallback and declining.
6. End-to-end comparison against the best single model.
7. Tiny decision agent as a WASM component.
8. Exploration share, and table updates from outcomes.
9. Step-level routing, only for step kinds where step 6 showed a gain.

## How to know it works

| Check | Kind |
|---|---|
| The profiler returns the right profile for every seed repository | Unit tests |
| An answer below its threshold is never used | Unit test of the router |
| A risky request reaches a person and no expert | Integration test |
| A declined task is retried once and no more | Integration test |
| The router track reproduces the first experiment's numbers for the tiny model | Regression check |
| Routing solves at least as many pilot tasks as the best single model, at equal budget | The end-to-end comparison; if it fails, do not ship the router |
