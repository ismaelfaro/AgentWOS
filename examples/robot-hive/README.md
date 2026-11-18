# Robot hive

Six robots work one map. They share knowledge, not orders: each one reads and
writes the same map, and each decides on its own.

- **Scouts** (3) go to the nearest unexplored cell that no other robot has
  claimed, so they spread out instead of crowding.
- **Miners** (3) go to the nearest known ore that is unclaimed, and scout when
  no ore is known yet.
- **Everyone** carries ore home when full, and goes home when its energy only
  just covers the way back.

Nobody messages anybody. The only shared thing is `Hive`: which cells are
explored, how much ore each known cell holds, which robot claimed which cell,
and how much ore reached the base.

```
robot ──sense──► Hive (shared map: explored, ore known, claims)
  │                  ▲
  └──decide, move────┘   each robot: its own role, own random stream, own ties
```

## What is independent and what is shared

| Per robot (independent) | In the hive (shared) |
|---|---|
| Role and how it ranks targets | Which cells are explored |
| Its own random stream and tie-breaks | How much ore each known cell holds |
| Its position, carried ore, energy | Which robot has claimed which cell |
| Which way it moves when two axes differ | Ore delivered to base |

A robot sees only its 3×3 neighbourhood. Everything farther away it knows
only from the map the others wrote.

## Run

No model and no WASM here: this is a plain native Rust crate with no
dependencies, so the logic is easy to read and to change.

```bash
cd examples/robot-hive
cargo run --release -- --ticks 200 --seed 7
```

- `--ticks N`: how many turns to run (default 200).
- `--seed S`: sets the ore map and every robot's random stream (default 7).
- `--out PATH`: where to write the log (default `runs/latest.js`).

It prints a summary: ore delivered, and the share of the map explored.

## View the run

Serve the repo root and open the dashboard:

```bash
tools/serve.py
# open http://localhost:8765/examples/robot-hive/dashboard/index.html
```

Run it from the repo root (from this folder: `../../tools/serve.py`). If port
8765 is taken by another program, pass a free one, for example
`tools/serve.py 8791`, and open the same path on that port.

The dashboard shows the shared map, every robot on it (scouts purple, miners
green, a gold dot when a robot carries ore), the stats, and what each robot did
on the selected tick. Use the slider or Play.

## Files

- `src/main.rs`: the world, the hive and the robot logic (`act`, `choose`,
  `sense`).
- `dashboard/index.html`: replays the run log.
- `runs/`: output log, git-ignored.

## Limits

- The map is a grid with six ore blobs. Ore is not placed by any robot's
  reasoning, so the hive cannot plan a route to ore it has never seen.
- Robots can stand on the same cell; there are no collisions.
- Robots are rule-based. Nothing here uses a language model, so the behaviour
  is repeatable for a given seed.
