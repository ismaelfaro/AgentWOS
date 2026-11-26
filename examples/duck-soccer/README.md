# Duck soccer: MicroDucks that learn to play, and to talk

Teams of three [MicroDuck](https://github.com/pollen-robotics/microduck) robots
learn to play soccer over generations, and invent words to call to each other
while they play. The question this example asks: **does talking make them play
better?** In this model, so far, it does not. The ducks learn to play, they learn
what their calls mean, and evolution learns which calls to stop making; but a
team that talks does not beat one that keeps quiet. The numbers are below.

```
generation ──► 8 teams play at once (threads), each a few matches
                  │   each duck: a policy of 11 numbers (how it plays, how much it heeds a call)
                  │   each duck: a vocabulary, learned while it plays
                  ▼
              best teams survive ──► elites, mutants, one newcomer ──► next generation
                  │
                  └── the best team vs the chasers, 100 matches talking and 100 muted
```

## The game

- **The pitch** is 10 m by 6 m with a 2 m goal at each end. Matches last 90 s.
- **Three ducks a side.** Each walks at up to 1.6 m/s, kicks the ball when it is
  within 0.4 m, shoots, passes and dribbles. Shots and passes are not perfect:
  the farther and harder, the wider they may go. A duck in the ball's path
  blocks it. The team that conceded kicks off; the other waits 2 m back.
- **Ducks see 3.5 m.** A MicroDuck looks through a camera and a short-range depth
  sensor in its head, so a duck here only knows about the ball, teammates and
  opponents within 3.5 m, and remembers the ball it saw for 3 s.
- **The chasers** are the fixed opponent every generation is measured against:
  three ducks that all run at the ball and shoot from mid-range. They never pass
  and never talk.

## How a duck plays

A duck's policy is 11 numbers between 0 and 1:

| | |
|---|---|
| Where it stands | how far up the pitch, how wide, how much it follows the ball, how deep it drops when the other team has the ball |
| With the ball | from how far it shoots and how hard, how readily it passes |
| Without the ball | how close the ball must be before it runs at it (the closest duck of a team always does) |
| Calls | how much it heeds "open", "mine" and "back" |

## How they talk

There are three things worth saying and four sounds (`quack`, `chirp`, `honk`,
`purr`). Nobody is given a meaning for any sound.

- **"Open"**: a free duck asks the duck with the ball for a pass. Since the
  carrier may not see it, the call tells it where the caller is.
- **"Mine"**: a duck running at the ball tells a teammate who is running at it
  too to leave it and cover, 1.5 m behind the ball.
- **"Back"**: the last duck back sees the other team attacking and calls the
  ducks ahead of it home; they may not see the attack.

Each duck starts with its own accent: a weight for every (meaning, sound) pair.
A speaker picks a sound by its weights; a listener reads it as the meaning it
weights highest and acts on it (as much as its policy says it should). The
speaker then sees whether the listener did what it asked (passed to it, left the
ball, ran back), and both adjust: success strengthens the pair on both sides, a
miss weakens it. That is the only feedback; no duck can see what another
understood.

Each team also has an inherited readiness to make each call, so evolution can
silence a word that hurts.

## Evolution

Each generation, eight teams each play four matches (the chasers twice, two other
teams), all eight on separate threads, and keep the words they learned. Fitness
is points (3 for a win, 1 for a draw) plus a quarter of the goal difference, per
match. The next generation is the two best teams unchanged (elites), five
changed copies of the top three (mutants: policy, words, learning rate and call
readiness), and one team that starts from nothing (a newcomer, as a control).

Every generation the best team then plays the chasers 100 times talking and 100
times muted (same team, calls off), and one match is recorded to watch.

## What it showed

Six seeds, 10 generations of 8 teams each:

| | Mean over six seeds |
|---|---|
| Wins against the chasers, generation 0 → 9 | 17% → 81% |
| Goal difference per match, generation 0 → 9 | −3.1 → +4.1 |
| Calls read the way they were meant (last three generations) | 55%, against 33% by chance |
| How readily the best team makes each call, at the end | "open" 27%, "mine" 6%, "back" 36% |

**Does talking help?** Two ways to ask it:

| | Talking | Not talking |
|---|---|---|
| The same team, calls on or off (wins, last three generations) | 83% | 85% |
| A lineage that evolves talking, against one that never talks (wins) | 83% | 88% |
| ... goal difference | +5.4 | +7.3 |

The lineage that talks was ahead on goal difference in 2 of the 6 seeds.

So: **the ducks learn to play** (in every seed), **they learn a language** (well
above chance, though not in every seed and not one word per meaning), and
**evolution learns to drop "mine"**, the call that hurt the most (it pulled a
duck off the ball against three chasers). But **talking does not make them win
more**.

What changed on the way, and why it matters:

1. The first version had a kick-off loop the evolution found and used: after a
   goal, the side standing nearer the middle scored again at once. Kick-off rules
   fixed it.
2. Learning words from "did my own action work?" taught wrong words, because the
   outcomes of different calls overlap. The speaker judging what the listener did
   fixed it: understanding went from below chance to about 80% while calls were
   frequent.
3. With every duck seeing the whole pitch, a call carries no news, so it can only
   get in the way. Limiting sight to 3.5 m gave calls something to say; talking
   still did not pay.
4. Calls were orders the listener had to obey. Making how much each duck heeds
   each call part of its genome brought the talking lineage from 62% to 83%
   wins, close to the silent one, but not past it.

Why talking does not pay here, as far as these runs say: the chasers are beaten
by positioning, which needs no words; every call adds parameters for evolution to
tune with only eight teams and ten generations; and a call that works only after
the language has settled is worth little to a team that has to win now.

## Run

Plain Rust, no model and no WASM. Each run takes about a second.

```bash
cd examples/duck-soccer
cargo run --release -- --pop 8 --gens 10 --eval 100 --seed 1                        # the talking lineage -> runs/latest.js
cargo run --release -- --pop 8 --gens 10 --eval 100 --seed 1 --talk 0 --out runs/silent.js   # a lineage that never talks
```

Options: `--pop` teams per generation, `--gens` generations, `--seconds` match
length, `--peers` other teams each team plays, `--eval` matches against the chasers
for each measurement, `--seed`, `--talk 0` to evolve without calls, `--out` the log.

## Watch it

```bash
tools/serve.py        # from the repo root
# open http://localhost:8765/examples/duck-soccer/dashboard/index.html
```

The dashboard replays the best team's match against the chasers for any
generation (pick it under the pitch), with the calls as speech bubbles and the
score. Beside it: the win rate by generation (talking, the same team muted, and the
silent lineage if `runs/silent.js` exists), how well calls were understood, how
readily each call is made, each duck's words, and a play-by-play.

## Publish

```bash
examples/duck-soccer/deploy.sh     # runs both lineages, then `wrangler deploy` (needs `npx wrangler login`)
```

The page is static: the dashboard and the two logs, served by Cloudflare as
`agentic-duck-soccer`.

## Limits

- **Not MicroDuck's simulator.** A flat 2D model: no legs, no servos, no falls.
  The ducks and the 3.5 m sight are inspired by MicroDuck, the physics is not.
- **One opponent.** Fitness and evaluation lean on the chasers; a team that beats
  them may not beat a team that passes.
- **Small evolution.** Eight teams and ten generations. Results vary a lot with the
  seed; read the six-seed table, not one run.
- **The brains are rules, not a model.** Unlike the [duck pond](../duck-pond), no
  language model is involved: an evolution needs millions of decisions.
