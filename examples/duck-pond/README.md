# Duck pond: [MicroDuck](https://github.com/pollen-robotics/microduck) bodies, [AgentWOS](https://github.com/ismaelfaro/AgentWOS) brains, [Needle](https://cactuscompute.com/needle) in the browser

**Online demo:** https://agentic-duck-pond.asst.workers.dev (runs in your browser: Needle loads once, about 35 MB)

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

## Talk: sounds that mean something (turn-based scenario)

Ducks can talk. Nobody gives them a shared language: each duck starts with its
own accent, a weight for every (meaning, sound) pair, and learns from what
happens.

- **Meanings** a duck can have: `come`, `play`, `rest`, `alarm`, `greet`. The
  body raises them: a duck quacking means `greet`, a duck heading for another
  duck means `come`, a duck next to the ball means `play`, a duck settling down
  means `rest`, a duck stopped by its head sensor means `alarm`.
- **Sounds**: `quack`, `chirp`, `honk`, `purr`, `hiss`. A duck picks the sound
  its accent gives most weight to for the meaning.
- **Hearing**: a duck within 3 m reads the sound as the meaning it weights
  highest for that sound, and acts on it: `come` walks to the speaker, `play`
  kicks the ball, `rest` naps, `alarm` moves away, `greet` quacks back.
- **Learning**: the listener checks whether the action worked (it got closer,
  kicked the ball, sat down, moved away, quacked). Success strengthens the pair
  for the listener; if the listener also understood what the speaker meant, the
  speaker's pair is strengthened too. A miss weakens the pair the listener used.

Over rounds the flock settles on shared words. The dashboard shows each sound
as a speech bubble, the round's talk (who said what, who understood and who
misread it), and the sound each duck now uses for each meaning, with its share.
The brain does not choose sounds yet: talk runs in the body, and the brain
hears what was said in its next round's prompt.

Limits: a listener can succeed for the wrong meaning (it walks to a speaker who
meant `rest`, and the walk counts as `come`); this is kept as it is in the
learning rule. Talk lives in the turn-based scenario, not in the browser stage
yet.

## Evolution: a language that survives

`evolve/` runs the talk system as an evolution. A generation is a population of
8 independent runs (or 4), all living at the same time on separate threads. Each
run is 100 rounds of 20 s with the four ducks, who learn what sounds mean as
they live. At the end the run with the best fitness survives and hands down
what it learned. The next generation is:

- **an elite**: the survivor's vocabulary, unchanged, played again with a new seed;
- **mutants**: the survivor's vocabulary softened and perturbed, with its learning
  parameters (reward, penalty, how often a duck speaks) mutated too;
- **a newcomer**: a flock that starts from nothing, as a control. If inherited
  knowledge did not matter, the newcomer would do as well as the rest.

10 generations are run by default.

**Fitness** is how well the ducks understand each other in the second half of
the run, scaled by how much they share the same words (a language the whole flock
uses). The first version scored understanding alone; it saturated in one
generation, because four ducks can read each other with four different
dictionaries. The shared-words term keeps selection working.

```bash
cd examples/duck-pond/evolve
cargo run --release -- --pop 8 --gens 10 --rounds 100 --round-s 20 --seed 1
# open http://localhost:8765/examples/duck-pond/dashboard/evolution.html
```

It takes about a second per run of 10 generations. `--out` changes the log;
`dashboard/evolution.html?file=NAME` opens another one in `runs/`.

**What it showed**, over 7 seeds (an evolution of 10 generations, 8 runs each):

| | |
|---|---|
| Best fitness, generations 0-2 → 7-9 | 0.73 → 0.81 (up or equal in all 7 seeds) |
| Inherited runs vs newcomers | 0.74 vs 0.48 (inherited ahead in all 7 seeds) |
| Shared words, best run, generations 0-2 → 7-9 | 0.66 → 0.71 |

So passing knowledge down clearly works. Selection improves things, but modestly
and noisily: after the first generations the fitness plateaus, and the ducks
rarely settle on one word per meaning. The learning rule (reward on success,
penalty on a miss) is the limit, and so is the metric.

**What is not the same as the pond with model brains.** The evolution uses a
fast rule-based brain (`evolve/src/brain.rs`) instead of a language model: it
needs millions of decisions, and a local model takes seconds for each. The
bodies and the talk are the same code (`scenario/src/body.rs`, `talk.rs`). The
model brains and the evolution have not been run together.

## Using the dashboard

The map stays in view while the panel on the right scrolls on its own. The
panel has four tabs: **Ducks** (one compact card each; the followed duck opens
up), **Talk** (what was said, lit as the playback reaches it), **Learning**
(understanding over rounds) and **Words** (each duck's vocabulary).

- **Play** starts the replay; the big button on the map does too.
- **Timeline**: a dot for every sound, coloured by the duck that made it, and a
  tick at every round boundary.
- **Click a duck on the map** to follow it: its card opens and the map rings
  it. Click the floor to let go.
- **Keys**: `Space` play or pause, `←` `→` previous or next round, `Shift` with
  an arrow moves 2 s, `Home` goes to the start, `S` changes speed
  (1×, 2.5×, 5×, 10×), `M` turns the sound on or off.
- The page watches `runs/latest.js` and shows a new run as soon as it is
  written; `?run=NAME` opens `runs/NAME.js`, and falls back to the latest.

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

## In the browser

The online demo is a second front end for the same bodies. Nothing runs on a
server except the page and the model file: the stage, the safety control and
[Needle](https://cactuscompute.com/needle) all run in your tab.

What changes from the native run:

- **The brain is Needle, not a chat model.** Each duck is an `actor` that turns
  one plain sentence into one tool call: `walk_to`, `kick_ball`, `nap`, `zoomies`,
  `look_around` or `stop`. `walk_to` takes a place: `cushion`, `sofa`, `table`,
  `plant`, `shelf` or `center`.
- **The sentence comes from you or from the duck.** Type one ("Pip walk to the
  cushion") to a duck by name, by colour, or to everyone. An idle duck puts its
  strongest urge into words ("Nilo take a nap") and the same brain acts on it.
  The urges are rules that weigh each duck's personality; the model does not
  decide what to want.
- **The four ducks** are Pip (blue, curious), Momo (grey, playful), Nilo (white,
  sleepy) and Lila (purple, chatty).
- **The same safety control** checks every decision, and the page shows the
  verdict next to what each brain decided.

### Using the page

- **Type a line** in the box under the map and press Enter or **Send**. Name a
  duck, its colour, or "everyone"; the chips under the box are lines that work.
  The line under the box says what will happen to what you type: it waits for
  the brains while they load, waits in line behind another duck's thought, or
  runs now.
- **Ducks** tab: one card per duck with what it is doing, its body's state, what
  its brain last decided and the safety verdict, and its energy. A duck whose
  brain is working shows "thinking…" on its card and three dots over its head;
  one that has not asked its brain yet says so. Click a card to ring the duck
  on the map.
- **Decisions** tab: every line, newest first, with the time, who did what and
  why, and the safety verdict. Your own lines are marked "you". The tab counts
  the decisions you have not seen.
- The map stays in view while the panel scrolls; on a phone the panel goes under
  the map. "How it works" folds out the three steps.

### Why the animation does not wait for the model

A decision from Needle takes about 0.8 s per duck addressed, and it blocks the
thread it runs on. So the page uses two workers:

- `web/src/worker.js` holds the bodies (the stage) and the safety control. It
  never waits for a model.
- `web/src/brain.js` holds Needle and the actor.
- The page passes each line to the brains, then hands what they decided to the
  bodies, which check it with the control and apply it (`web/src/split.js` has
  the two halves).

The bodies tick ten times a second on a timer, and the page draws every display
frame in between, easing from one body frame to the next. The page opens as soon
as the bodies are ready, before the 35 MB model has finished loading, and a bar
under the header shows the model loading. The duck cards are built once and
updated in place.

Before, the bodies and the model shared one worker and the page stopped asking
for frames while a line ran, so the pond froze for as long as the model took.
Measured in a browser with the same line ("Everyone walk to the table") in the
same minute:

| | One worker (before) | Two workers (now) |
|---|---|---|
| Body frames during an 11 s decision | 1 | 94 |
| Longest freeze | 12.3 s | 0.23 s |

A Safari timeline of the old page also showed the cards being rebuilt about ten
times a second, each time invalidating layout for the whole page; in 6 s it
created and removed 208 card nodes, each with a new canvas. Now none are.

Build and test, from the repo root. This needs Node and Rust with `wasm32-wasip2`.
The first build downloads the pinned Cactus engine and checks its hash.

```bash
examples/duck-pond/build-web.sh
tools/serve.py
# open http://localhost:8765/examples/duck-pond/web/dist/index.html
(cd examples/duck-pond/web && node test/smoke.mjs)     # the stage, the control and the two-worker split, with the real model
```

The page fetches the 35 MB Needle model once and caches it. If `web/vendor/needle3.cact`
exists when you build, it is copied next to the page and served from there;
otherwise the page falls back to Hugging Face. The smoke test downloads the model
into `web/vendor/` on its first run.

`examples/duck-pond/deploy.sh [worker-name]` builds the page and publishes it to
Cloudflare with Wrangler (`npx wrangler login` first). A small Worker
(`worker/edge.js`) serves the model, so it is not uploaded with the page. The
script writes an `.assetsignore` so Wrangler's own files and the Worker's code
are not published as static files.

## Files

- `scenario/src/lib.rs`: the `scenario` interface: prompts, parsing, the round
  log.
- `scenario/src/body.rs`: the room and the bodies. The browser stage reuses it.
- `scenario/src/talk.rs`: sounds, hearing and learning, in the turn-based scenario.
- `evolve/`: the evolution (a native crate that shares `body.rs` and `talk.rs`).
- `controls/duck-safety/`: the control, used by both front ends.
- `dashboard/index.html`: the replay of a native run.
- `dashboard/evolution.html`: the evolution dashboard.
- `manifest.json`: scenario, control, model, rounds, log path.
- `stage/`: the live stage for the browser (the `stage` interface): the six
  tools, the places and the urges.
- `web/`: the page (`src/`: `main.js` the page, `worker.js` the bodies, `brain.js`
  the model, `split.js` the two halves of a cue), its smoke test (`test/`) and its
  bundler setup.
- `build-web.sh`: builds the browser version into `web/dist/`.
- `deploy.sh` and `worker/edge.js`: publish to Cloudflare, and the Worker that
  serves the model.
- `runs/`: output, git-ignored.

## Limits

- **This is not MicroDuck's simulator.** MicroDuck's own twin runs the real
  daemons against a MuJoCo body. Here the body is a flat 2D model written for
  this example: no legs, no servos, no IMU, no ONNX policies. It borrows
  MicroDuck's rules and numbers, not its code.
- **It does not drive a real duck.** Nothing here talks to `robotd`.
- A brain decides once per 6 s round, far slower than `autod`'s 50 Hz loop. The
  reflexes that need speed live in the body for that reason. In the browser, an
  idle duck asks its brain when its body has an urge, so the ducks do not decide in
  lockstep.
- In the browser there is one brain thread, so lines are handled one at a time:
  a line for everyone takes four ducks' worth of thinking, and the next waits.
  A duck's own urge that waits more than 12 s is dropped, since the duck has
  moved on. The animation does not wait either way.
- Behaviour depends on the model. A small model repeats itself: in test runs
  with `llama-3.1-8b`, some ducks mostly quacked or looked around.
- The chance of getting up after a fall is a fixed number, not physics.

## Links

- [AgentWOS](https://github.com/ismaelfaro/AgentWOS): this repository, the WebAssembly agent OS.
- [Needle](https://cactuscompute.com/needle) by Cactus Compute: the tool-calling model that runs in the browser.
- [MicroDuck](https://github.com/pollen-robotics/microduck) by Pollen Robotics: the robot whose logic the bodies follow.
