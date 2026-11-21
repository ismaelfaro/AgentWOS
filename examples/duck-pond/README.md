# Duck pond: [MicroDuck](https://github.com/pollen-robotics/microduck) bodies, [AgentWOS](https://github.com/ismaelfaro/AgentWOS) brains, [Needle](https://cactuscompute.com/needle) in the browser

**Online demo:** https://agentic-duck-pond.asst.workers.dev (runs in your browser: Needle loads once, about 35 MB)
· **New: [Duck soccer](soccer/README.md)**, two against two: https://agentic-duck-pond.asst.workers.dev/soccer.html

Four [MicroDuck](https://github.com/pollen-robotics/microduck) robots share an
apartment. Each duck's **brain is an AgentWOS agent**; its **body follows
MicroDuck's logic**. An OS control sits between the two.

```
senses ──► brain (AgentWOS agent + model)      one per duck
              │  {"state": "wander", "goal": [x, y], "thought": "..."}
              ▼
        duck-safety.wasm      allow | modify | block | flag
              │
              ▼
        duck_pond.wasm        the bodies: gait, ToF reflex, falls, get-up
              │
              ▼
        runs/latest.js ──► dashboard/index.html
```

On a real MicroDuck the brain is a daemon, `autod`: it reads the robot's state
and the head sensor, picks a state (chill, wander, zoomies, nap...) and sends
intents to `robotd`, which owns walking, safety and falls. This example keeps
that split and swaps the brain: every round an AgentWOS agent gets the duck's
senses as a message and answers with a state and a goal.

## What the brain sees and decides

Each round (6 seconds of the ducks' lives) a brain is told: where the duck is
and where it faces, its energy, what the head sensor sees ahead, where the ball
and the other ducks are, which quacks it heard, what happened last round, its
last few states, and the place it knows least.

It answers with one of eight states, a subset of `autod`'s:

| State | What the body does |
|---|---|
| `chill` | Stands and rests |
| `look_around` | Stands and looks |
| `quack` | Quacks; ducks within 3 m hear it next round |
| `wander` | Walks to the goal |
| `turn` | Turns to face the goal |
| `zoomies` | Runs at full speed with jerky turns; tiring |
| `ball` | Goes to the ball and kicks it |
| `nap` | Sits down and recovers energy |

The four ducks have different personalities (curious, playful, sleepy, chatty)
and nothing else in common: one agent instance each, no shared memory.

If a brain does not answer, or answers something that is not a decision, the
duck stands still. That is `robotd`'s deadman: no intent, no motion.

## What the body does on its own

The body is the `scenario` component (`scenario/src/body.rs`). The brain cannot
override any of this:

- **Gait.** Speeds are the pad's: 0.3 m/s and 1.5 rad/s. A start from a
  standstill needs a short burst at full speed, small commands make it stand,
  and it walks slower than it is asked to.
- **ToF reflex.** Something in the corridor ahead within 0.45 m slows the duck
  and steers it away; within 0.15 m it stops and spins away.
- **Falls.** Nothing sees sideways or behind. A bump faster than 0.18 m/s
  knocks the duck over.
- **Get-up watch.** A fallen duck tries to stand. Still down after 5 s, the
  policy is cut for 2 s and it tries once more. Still down after that, it goes
  limp and needs a hand; a limp duck stays limp.
- **Energy**, 0 to 1. Walking, the ball and zoomies burn it; standing and naps
  refill it.
- **Novelty grid.** Each duck remembers how long it spent in each 0.5 m cell;
  a walk with no goal goes to the place it knows least.

The constants come from MicroDuck's `autod/src/obstacle.rs`,
`docs/design/autonomous.md` and `docs/design/simulation.md` (Apache-2.0).

## The control: duck safety

`controls/duck-safety` inspects every decision before the body runs it:

1. **Block** any new state for a duck that is fallen or limp: the get-up watch
   owns the body.
2. **Modify** a moving state into a nap below 20% energy.
3. **Modify** zoomies into a wander when the sensor sees something within
   0.45 m ahead.
4. **Flag** a goal outside the room. The body then clamps it to the walls.

## Run

Requires Rust with `wasm32-wasip2` and the host built from the repo root
(`cargo build --release`). Run these from the repo root:

```bash
examples/duck-pond/build.sh
./target/release/agentwos-host --manifest examples/duck-pond/manifest.json
```

The manifest runs 12 rounds with `local/google/gemma-4-e4b` (LM Studio on
`localhost:1234`). To use OpenRouter instead, set `OPENROUTER_API_KEY` and pass
a model:

```bash
./target/release/agentwos-host --manifest examples/duck-pond/manifest.json \
  --model openrouter/meta-llama/llama-3.1-8b-instruct --colony 12
```

`--colony N` sets the number of rounds. With no model reachable the run still
completes: every duck stands still, and the log records each brain's error.

## View the run

```bash
tools/serve.py
# open http://localhost:8765/examples/duck-pond/dashboard/index.html
```

If port 8765 is taken, pass another: `tools/serve.py 8791`.

The dashboard draws the apartment from above and replays the run: each duck
with its trail, the dashed line to the goal its brain chose, quack rings, the
ball, naps and falls. A card per duck shows what its brain thought, what it
asked for and what the body ran, the safety verdict, energy and what happened.
The page reloads the log while a run is going.

## Files

- `scenario/src/lib.rs`: the `scenario` interface: prompts, parsing, the round
  log.
- `scenario/src/body.rs`: the room and the bodies.
- `controls/duck-safety/`: the control.
- `dashboard/index.html`: the replay.
- `manifest.json`: scenario, control, model, rounds, log path.
- `runs/`: output, git-ignored.

## Limits

- **This is not MicroDuck's simulator.** MicroDuck's own twin runs the real
  daemons against a MuJoCo body. Here the body is a flat 2D model written for
  this example: no legs, no servos, no IMU, no ONNX policies. It borrows
  MicroDuck's rules and numbers, not its code.
- **It does not drive a real duck.** Nothing here talks to `robotd`.
- A brain decides once per 6 s round, far slower than `autod`'s 50 Hz loop. The
  reflexes that need speed live in the body for that reason.
- Behaviour depends on the model. A small model repeats itself: in test runs
  with `llama-3.1-8b`, some ducks mostly quacked or looked around.
- The chance of getting up after a fall is a fixed number, not physics.

## In the browser

`build-web.sh` builds two pages that run everything in one tab, with no server
after the first load: the pond (`web/dist/index.html`) and the football match
(`web/dist/soccer.html`, see [soccer/README.md](soccer/README.md)). Each duck's
brain is the shared `actor` component with Needle. The bodies run in a worker
on their own clock, ten ticks a second, while a brain thinks; the page draws the
poses in between at screen rate.

Needle answers in about 150 ms. All ducks use the same system prompt, so the
engine prepares it once (about 700 ms on the first call) and reuses it.

```bash
examples/duck-pond/build-web.sh
(cd examples/duck-pond/web && node test/smoke.mjs)   # the pond, with the real model
(cd examples/duck-pond/web && node test/soccer.mjs)  # the match
examples/duck-pond/deploy.sh                         # publish to Cloudflare (needs wrangler login)
```

## Talking: the ducks learn words

Nobody gives the ducks a language. Each duck has four sounds (`chirp`, `trill`,
`honk`, `purr`) and no meanings. The rules:

- **Speaking.** A duck that starts walking to a place says one sound for it: the
  sound it has learned for that place, or a random one one time in five. That is
  how a word gets tried, and how new ones get found.
- **Hearing.** Ducks within 3 m of a speaker hear the sound, and count it with the
  place it came with. The place is in the room, not in the sound.
- **Learning.** A sound means a place once a duck has heard it with that place
  clearly more often than with any other. A walk started by a heard sound, and
  arrived at, teaches that sound again.
- **Following.** A duck that hears a sound it has learned to mean a place
  sometimes asks its brain to walk there. The brain (Needle) still decides how
  to act; the word only gives it a reason.

The page shows each duck's vocabulary and how often pairs of ducks agree. A
sound with no clear meaning is shown as "—": the ducks have heard it, but it
does not mean anything yet.

Limits: the words are learned from places only, not from the ball or from
feelings. Ducks do not talk to a person: a person speaks to ducks, and the ducks
speak to each other.

## Links

- [AgentWOS](https://github.com/ismaelfaro/AgentWOS): this repository, the WebAssembly agent OS.
- [Needle](https://cactuscompute.com/needle) by Cactus Compute: the tool-calling model that runs in the browser.
- [MicroDuck](https://github.com/pollen-robotics/microduck) by Pollen Robotics: the robot whose logic the bodies follow.
