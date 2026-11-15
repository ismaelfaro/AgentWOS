---
name: agentwos
description: Procedures for working on AgentWOS, the WebAssembly agent operating system in this repository. Use when adding or changing a WASM component (plugin agent, tool, control, scenario, stage), changing wit/world.wit, building or verifying an example (CLI harness, Mars colony, Tab Jam, Tab Swarm), writing a feature design (concept, build notes, stories), or committing and publishing work here.
---

# Working on AgentWOS

Read `AGENTS.md` at the repository root first: it has the layout, the commands
and the rules. This skill adds the step-by-step procedures.

## Before anything

1. `git status -sb` and `git branch --show-current`. Other sessions share this
   checkout: note the branch, and treat changes you did not make as someone
   else's.
2. If the task names a design term (intent, grant, mandate, guard, ledger,
   registry, plane), check whether it exists in code: `grep -rn "<term>" host/src wit web/kernel`.
   Most of `design/` is proposal. Say so instead of assuming.

## Pick the procedure

| Task | Read |
|---|---|
| New plugin agent, tool, control, scenario or stage; or a change to `wit/world.wit` | [references/components.md](references/components.md) |
| Build, run or verify anything | [references/verify.md](references/verify.md) |
| Design a feature | [references/design-docs.md](references/design-docs.md) |
| Commit, push, publish | [references/publishing.md](references/publishing.md) |

## Rules that are easy to break

- New behaviour is a component, not host code. The host loads, links, checks
  and records.
- Never copy `wit/world.wit` into a crate. Point `wit_bindgen::generate!` at it
  with a relative path.
- A component that fails for lack of a grant is working as designed. Find the
  narrowest grant; do not hand it everything.
- Tool declarations sent to a model keep their key order (`serde_json` with
  `preserve_order`).
- Do not rename code to match design vocabulary unless asked.
- Report what you could not verify. Sound, touch input and models you could not
  run are the usual ones.
