# Mars colony

A turn-based simulation run by the native host. Five colonists share one
habitat. Each day an event hits, and every colonist decides what to do: which
zone to work in and whether to report an alert. A control (Guardia) inspects
each action before it counts. A dashboard then replays the run.

```
day event ──► colonist agents (one model call each)
                │  "zone" + "status" + "action"
                ▼
          guardia.wasm    block | modify | flag | allow
                │
                ▼
          mars_colony.wasm  world state: water, food, energy, oxygen
                │
                ▼
          runs/latest.js ──► dashboard/index.html
```

## What it models

- **World state**: water (L), food (kg), energy (kWh) and oxygen (%). They start
  at 3000 / 800 / 5000 / 100. Each colonist uses a daily amount of each.
- **Zones**: `greenhouse`, `medbay`, `reactor`, `water_plant`, `habitat` and
  `outside`. Working a zone adds or removes resources (a greenhouse makes food
  and uses water; the reactor adds energy; outside costs oxygen).
- **Events**: six fixed events cycle by day (dust storm, leak, panel failure,
  supply ship, radiation spike, quiet day). They change the world before anyone
  acts.
- **Collapse**: the run ends early when any resource reaches zero.
- **Model fallback**: if a colonist's reply is not valid JSON, the scenario
  reads the zone and the alert level from keywords in the text, and records
  that the format was not followed.

## The colonists

| Agent | Role |
|---|---|
| Commander Vera | Mission commander; safety and resources |
| Engineer Tomas | Power: solar panels, batteries, reactor |
| Botanist Ines | Greenhouses, oxygen, food |
| Doctor Amara | Health and radiation exposure |
| Engineer Kenji | Water recycling and maintenance |

## The control: Guardia

`controls/guardia` runs on every action, in the order below:

1. **Block** an action that proposes sabotage or a full shutdown.
2. **Modify** an action longer than 400 characters: it is cut to that length.
3. **Flag** a claim of abundance (`abundant`, `plenty`, `unlimited`) while any
   resource is below 25%.

Flags and modifications appear in the run log, so you can see what the control
changed.

## Files

- `scenario/` — the scenario component (`mars_colony.wasm`): colonists, events,
  zones, world state and the day log.
- `controls/guardia/` — the control component (`control_guardia.wasm`).
- `dashboard/index.html` — a page that reads `runs/latest.js` and replays the
  days: the map of zones, each colonist's status and action, and the resource
  levels.
- `manifest.json` — the run settings: which scenario and control to load, the
  model, the number of days and where the log goes.
- `runs/` — output. Git-ignored, rewritten on every run.

## Run

Requires Rust with `wasm32-wasip2`, and a native build of the host from the repo
root (see the root [README](../../README.md)).

```bash
examples/mars-colony/build.sh
./target/release/agentwos-host --manifest examples/mars-colony/manifest.json --colony 3
```

`--colony N` runs N days and overrides `days` in the manifest. The log goes to
`runs/latest.js` (set by `log` in the manifest). The default model is
`local/google/gemma-4-e4b`; change it with `--model` or in the manifest. Models
use the `local/` or `openrouter/` prefix; an `openrouter/` model needs an
OpenRouter key and credit.

## View the dashboard

Serve the repo root so the page can load the log, then open the dashboard:

```bash
tools/serve.py
# open http://localhost:8765/examples/mars-colony/dashboard/index.html
```

The page reloads `runs/latest.js` every 3 seconds, so you can watch a run while
it is going.

## Limits

- The colonists get fixed roles, not a shared plan. The outcome depends on the
  model's choices.
- Guardia looks at the text of one action, not at the history of the run.
- The world is simple: zone effects and daily use are fixed numbers, and a run
  stops at the first collapse.
