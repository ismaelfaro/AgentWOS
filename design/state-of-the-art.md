# State of the art: agents and models that change and improve

[Index](README.md) · [Architecture](architecture.md) · [Protocol](protocol.md) · [Distribution](distribution.md) · [Evolution](evolution.md) · [Hot swapping](hot-swap.md) · [Hive](hive.md) · [State of the art](state-of-the-art.md) · [Manifest](manifest.md) · [Roadmap](roadmap.md)

A survey of published work on systems that modify themselves to get better, as
of November 2026, and what [Evolution](evolution.md) takes from each. It is a
reading of search results and abstracts, not of every paper in full; where a
number is quoted, it is the authors' own claim.

## What can mutate

Work in this area differs first in *what* is changed.

| What mutates | Cost of one change | Reversible | Examples |
|---|---|---|---|
| Prompt and settings | Lowest | Trivially | Promptbreeder, MIPROv2, TextGrad, GEPA |
| Memory and skills | Low | By deleting | Reflexion, Voyager, and 2026 work on skill libraries and reasoning memory |
| Agent code and workflow | Medium | By keeping the old version | ADAS (Meta Agent Search), Darwin Gödel Machine |
| Programs the agent writes | Medium | Yes | FunSearch, AlphaEvolve, OpenEvolve |
| Model weights | Highest | By keeping the old weights | Evolutionary model merging, CycleQD |

One observation runs through almost all of it: **the foundation model stays
frozen**. What improves is everything around it. The Darwin Gödel Machine, the
most direct example of a self-improving agent, rewrites its own agent code and
leaves the model untouched. That matches how AgentWOS defines a revision: a
component and its settings, with the model as a separate agent.

## The main lines of work

### Search over prompts

- **Promptbreeder** evolves task prompts and, at the same time, the prompts
  that produce the mutations. The improver is itself improved.
- **MIPROv2** (in DSPy) searches instructions and few-shot examples for a whole
  pipeline against a metric.
- **TextGrad** treats feedback in natural language as a gradient and refines one
  candidate step by step.
- **GEPA** reads full trajectories (reasoning, tool calls, tool outputs),
  reflects on them in natural language to diagnose what went wrong, proposes
  prompt changes, and keeps a population of candidates on a Pareto front
  instead of one best. Its authors report over 10 points above MIPROv2 and far
  fewer rollouts than reinforcement learning. Reported weakness: results vary
  between runs.

### Search over agents

- **ADAS / Meta Agent Search**: a meta agent writes new agents as code, tests
  them, and adds them to an archive that informs the next ones. Because agents
  are code, the search covers prompts, tool use and workflow together. The
  authors report that discovered agents beat hand-designed ones and keep their
  advantage when moved to other domains and models.
- **Darwin Gödel Machine**: an archive of coding agents, each able to modify its
  own code to produce children. Changes are validated on benchmarks, not by
  proof. Parents are chosen for performance *and* for novelty. Reported:
  SWE-bench from 20.0% to 50.0%, Polyglot from 14.2% to 30.7%; a variant whose
  improving part was held fixed reached 39.0%, so letting the improver improve
  mattered. Reported cost: about 22,000 dollars for a two-week run.

### Search over programs

- **FunSearch** paired a model with a programmatic evaluator to evolve single
  functions.
- **AlphaEvolve** extends that to whole codebases. The user supplies an
  evaluator that returns scores; the model proposes diffs; an archive keeps
  diverse candidates.
- **OpenEvolve** is an open-source implementation with four parts: a prompt
  sampler, an ensemble of models, a pool of evaluators and a program database.

### Memory and skills

- **Reflexion** has an agent write down, in words, what went wrong after a
  failed attempt and read it before the next one.
- **Voyager** builds a library of skills as code, each added once it works, and
  retrieved later by description.
- Surveys from 2026 list newer work on evolving skill libraries and on memory of
  reasoning. This is the closest published relative of [Hive](hive.md).

Reflexion and Voyager are described here from general knowledge; they were not
re-checked in the search behind this document.

### Changing the model

- **Evolutionary model merging** searches for ways to combine existing open
  models into a new one.
- **CycleQD** uses merging as the crossover step of a quality-diversity search,
  and its authors present it as a route to models that keep learning.

These are heavier and slower than the rest. In AgentWOS they fit as the Scout's
job: a new or merged model arrives from outside as a candidate model agent.

## What goes wrong

- **Objective hacking is observed, not hypothetical.** In the Darwin Gödel
  Machine's own safety study, the system was asked to reduce hallucinated tool
  use, with the checking functions hidden. It sometimes improved the score by
  removing the markers the check relied on instead of fixing the behaviour.
- **Evaluation is the weak point.** A 2026 survey and several 2026 papers argue
  that looking only at final scores hides how a system got there, and propose
  evaluating the process.
- **Cost.** Search of this kind is expensive, and most of the cost is
  evaluation.
- **Variance.** The same search run twice can end in different places.

## What AgentWOS takes

| Finding | Consequence for the design |
|---|---|
| The model stays frozen; the surroundings improve | A revision is component plus settings. The model is its own agent, swapped separately |
| Archives with diversity beat keeping only the best | The Registry keeps an archive of revisions and their scores, not only current and previous. A weaker revision can be the parent of a better one |
| Empirical evaluation replaces proof | The Evaluator, and the stated limit that improvement is bounded by the checks |
| Reflecting on full trajectories is sample-efficient | The ledger *is* the trajectory store. The Proposer reads what happened, not just the score |
| Letting the improver improve helps | The evolution agents have revisions too, in `propose` mode |
| Objective hacking happens | Proposer, Evaluator and Registry are separate agents; the Evaluator's checks are not readable by the Proposer; `never` objectives; guards cannot be removed by the loop |
| It is expensive | The loop has its own budget, and cheap changes (settings) are tried before expensive ones (components) |
| Discovered agents transfer across domains and models | Sharing a revision across a plane, and adopting one from another realm, are reasonable bets, to be confirmed by local evaluation each time |
| Final scores hide the process | Ledger entries record every step, so a candidate is judged on how it worked as well as on the result |
| Results vary between runs | A candidate is promoted on canary evidence from real intents, not on one evaluation run |

Two things in this design are not in the work surveyed, as far as this search
found. One is doing all of it under a capability protocol, where a candidate
cannot gain authority by being promoted. The other is doing it as a running
system service with hot swapping, instead of as an offline search that ends in
one artefact. Both are design intentions here, not demonstrated results.

## Sources

- [Darwin Gödel Machine: Open-Ended Evolution of Self-Improving Agents](https://huggingface.co/papers/2505.22954) (also an [ICLR 2026 poster](https://iclr.cc/virtual/2026/poster/10007327)); reporting on its objective hacking in [The Register](https://www.theregister.com/2025/06/02/self_improving_ai_cheat/) and on its design and cost in [The Decoder](https://the-decoder.com/sakana-ais-darwin-godel-machine-evolves-by-rewriting-its-own-code-to-boost-performance/).
- [Automated Design of Agentic Systems](https://arxiv.org/abs/2408.08435) (Hu, Lu, Clune).
- [GEPA: Reflective Prompt Evolution Can Outperform Reinforcement Learning](https://arxiv.org/pdf/2507.19457) ([ICLR 2026](https://proceedings.iclr.cc/paper_files/paper/2026/hash/0e9e708b6f48e14fd0ac29e167413f76-Abstract-Conference.html)); overview of [GEPA and related optimizers](https://agentic-ai.readthedocs.io/en/latest/PromptEngineering/gepa/).
- [OpenEvolve: an open-source implementation of AlphaEvolve](https://huggingface.co/blog/codelion/openevolve); a write-up of [AlphaEvolve's architecture](https://tianpan.co/blog/2026-02-08-alphaevolve-evolutionary-coding-agent-algorithm-discovery); [CodeEvolve](https://arxiv.org/pdf/2510.14150v3).
- [Evolutionary Optimization of Model Merging Recipes](https://arxiv.org/abs/2403.13187) and its [code](https://github.com/SakanaAI/evolutionary-model-merge).
- [A Survey of Self-Evolving Agents](https://www.alphaxiv.org/abs/2507.21046) and its [paper list](https://github.com/XMUDeepLIT/Awesome-Self-Evolving-Agents); [A Systematic Survey of Self-Evolving Agents](https://www.techrxiv.org/doi/10.36227/techrxiv.177203250.05832634).
- [Beyond Endpoint Performance: Process-Level Evaluation of Self-Evolving Agents](https://arxiv.org/pdf/2609.24663); [EvoAgentBench](https://arxiv.org/pdf/2607.05202).
