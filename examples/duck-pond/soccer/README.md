# Duck soccer

**Online demo:** https://agentic-duck-pond.asst.workers.dev/soccer.html (runs in your browser: Needle loads once, about 35 MB)

Two teams of [MicroDuck](https://github.com/pollen-robotics/microduck) robots
play two against two. Each duck's brain is an [AgentWOS](https://github.com/ismaelfaro/AgentWOS)
actor running [Needle](https://cactuscompute.com/needle), a 35 MB tool-calling
model. Everything runs in one browser tab.

```
"Pip attack"  (you, coaching)   or   "Pip attack" (Pip's own urge)
        │
        ▼  actor.wasm + Needle: one sentence -> one tool call
  attack | defend | pass_ball | stop
        │
        ▼
  referee.wasm          allow | modify | block
        │
        ▼
  duck_soccer_stage.wasm   footwork, then MicroDuck's body: gait, falls, get-up
```

| Team | Ducks | Defends |
|---|---|---|
| Sky | Pip (blue), Lila (purple) | the left goal |
| Sun | Momo (grey), Nilo (white) | the right goal |

## Who decides what

- **The brain picks a role.** Needle turns a sentence into one of four calls:
  `attack`, `defend`, `pass_ball`, `stop`. It does not plan moves.
- **The body plays it.** The stage turns a role into footwork: get behind the
  ball on the line to the target, then walk through it; or stand at the edge of
  the goal area between the ball and the goal. The duck's body (shared with the
  pond, `scenario/src/body.rs`) keeps MicroDuck's gait, falls and get-up watch.
  Chasing the ball turns off the ToF reflex, as MicroDuck's own soccer does;
  otherwise two ducks at one ball would only dodge each other.
- **The duck speaks for itself.** Every two or three seconds each duck looks at
  the game: the one closer to the ball wants to attack, its teammate to defend,
  a pressed attacker may pass, a tired duck stops. When that changes its role,
  the duck says it ("Lila defend the goal") and its brain acts on the sentence,
  the same way as on yours.
- **You coach.** Type a line or click a suggestion: a name (`Pip`), a colour
  (`the purple duck`), a team (`Sky`, `Sun`) or `Everyone`. A coached duck keeps
  your order for ten seconds before its own urges come back.

## The referee

`controls/referee` is an OS control. Every call a brain makes passes through it:

1. **Block** any order but `stop` to a duck that is on the floor.
2. **Block** `attack` and `pass_ball` until the kick-off whistle.
3. **Modify** a second `attack` in the same team into `defend`: one attacker each.

The stage also referees what is not a call: it drops a ball stuck at a wall
for five seconds back at the centre, and sets a duck that cannot get up back on
its feet after six.

## Run

From the repo root, with Rust (`wasm32-wasip2`) and Node.js:

```bash
examples/duck-pond/build-web.sh
tools/serve.py
# open http://localhost:8765/examples/duck-pond/web/dist/soccer.html
```

Check it with the real model in Node (about two minutes):

```bash
(cd examples/duck-pond/web && node test/soccer.mjs)
```

It checks every order and suggestion, the referee's three rules, that a
coached duck holds its order, and plays twenty minutes on the ducks' own urges:
every urge must reach the brain as the right call, and goals must be scored.

## Limits

- The footwork is rules, not learned. Ducks do not dribble, and a shot is a
  walk through the ball.
- The physics is the pond's flat 2D model: no MuJoCo, no legs, a ball that rolls
  and bounces off walls.
- Needle reads the order without the duck's name (`strip_aliases` in the brief):
  with a name in front, it sometimes took `Pip attack` for a pass.
- Ducks bump and fall more than real ones would: about one fall a minute.
