//! Robot hive: several robots share one map and act on their own logic.
//!
//! The only thing they share is `Hive`, the common knowledge: which cells are
//! explored, how much ore each known cell holds, and which robot claimed which
//! target. No robot messages another. Each one has its own role, its own
//! random stream and its own tie-breaks, so the hive's behaviour comes from
//! many independent decisions reading the same map.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

const W: usize = 24;
const H: usize = 16;
const BASE: (usize, usize) = (12, 8);
const CAPACITY: u32 = 5;
const MAX_ENERGY: i32 = 60;
const ROLES: [Role; 6] = [Role::Scout, Role::Scout, Role::Scout, Role::Miner, Role::Miner, Role::Miner];

#[derive(Clone, Copy, PartialEq)]
enum Role {
    Scout,
    Miner,
}

impl Role {
    fn name(self) -> &'static str {
        match self {
            Role::Scout => "scout",
            Role::Miner => "miner",
        }
    }
}

/// xorshift64: each robot owns one, so no two robots share a random stream.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

#[derive(Clone, Copy, Default)]
struct Cell {
    explored: bool,
    ore_known: u32,
    claimed_by: Option<usize>,
}

/// Shared knowledge. Every robot reads and writes it; nothing else.
struct Hive {
    cells: Vec<Cell>,
    delivered: u32,
}

struct Robot {
    id: usize,
    name: String,
    role: Role,
    pos: (usize, usize),
    carrying: u32,
    energy: i32,
    target: Option<(usize, usize)>,
    rng: Rng,
}

fn idx(p: (usize, usize)) -> usize {
    p.1 * W + p.0
}

fn dist(a: (usize, usize), b: (usize, usize)) -> i32 {
    (a.0 as i32 - b.0 as i32).abs() + (a.1 as i32 - b.1 as i32).abs()
}

/// Ore is a few blobs of cells away from the base. The seed fixes the map.
fn place_ore(rng: &mut Rng) -> Vec<u32> {
    let mut ore = vec![0u32; W * H];
    for _ in 0..6 {
        let cx = 2 + rng.below((W - 4) as u64) as usize;
        let cy = 2 + rng.below((H - 4) as u64) as usize;
        let amount = 6 + rng.below(10) as u32;
        for dy in -1i32..=1 {
            for dx in -1i32..=1 {
                let (x, y) = ((cx as i32 + dx) as usize, (cy as i32 + dy) as usize);
                if dist((x, y), BASE) > 2 {
                    ore[y * W + x] = amount;
                }
            }
        }
    }
    ore
}

fn release(r: &Robot, hive: &mut Hive) {
    if let Some(t) = r.target {
        let c = &mut hive.cells[idx(t)];
        if c.claimed_by == Some(r.id) {
            c.claimed_by = None;
        }
    }
}

fn claim(r: &mut Robot, hive: &mut Hive, t: Option<(usize, usize)>) {
    release(r, hive);
    r.target = t;
    if let Some(t) = t {
        hive.cells[idx(t)].claimed_by = Some(r.id);
    }
}

/// Looks at its 3x3 neighbourhood and writes what it sees into the shared map.
fn sense(r: &Robot, hive: &mut Hive, ore: &[u32]) {
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            let x = r.pos.0 as i32 + dx;
            let y = r.pos.1 as i32 + dy;
            if x < 0 || y < 0 || x >= W as i32 || y >= H as i32 {
                continue;
            }
            let i = y as usize * W + x as usize;
            hive.cells[i].explored = true;
            hive.cells[i].ore_known = ore[i];
        }
    }
}

/// Each role picks from the shared map with its own noise. Scouts spread out
/// over unexplored cells; miners go for the nearest known ore, and scout when
/// they know none.
fn choose(r: &mut Robot, hive: &Hive) -> Option<(usize, usize)> {
    let free = |c: &Cell| c.claimed_by.is_none() || c.claimed_by == Some(r.id);
    let mut best: Option<(i32, (usize, usize))> = None;

    let consider = |score: i32, p: (usize, usize), best: &mut Option<(i32, (usize, usize))>| {
        if best.is_none_or(|(s, _)| score < s) {
            *best = Some((score, p));
        }
    };

    if r.role == Role::Miner {
        for (i, c) in hive.cells.iter().enumerate() {
            if c.ore_known > 0 && free(c) {
                let p = (i % W, i / W);
                let noise = r.rng.below(3) as i32;
                consider(dist(r.pos, p) * 4 + noise, p, &mut best);
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
    }

    for (i, c) in hive.cells.iter().enumerate() {
        if !c.explored && free(c) {
            let p = (i % W, i / W);
            let noise = r.rng.below(4) as i32;
            consider(dist(r.pos, p) + noise, p, &mut best);
        }
    }
    best.map(|(_, p)| p)
}

/// One step toward the target. When both axes differ, the robot's own random
/// stream decides which axis it moves along first.
fn step_toward(pos: (usize, usize), target: (usize, usize), rng: &mut Rng) -> (usize, usize) {
    let mut moves = Vec::with_capacity(2);
    if pos.0 != target.0 {
        moves.push((if target.0 > pos.0 { pos.0 + 1 } else { pos.0 - 1 }, pos.1));
    }
    if pos.1 != target.1 {
        moves.push((pos.0, if target.1 > pos.1 { pos.1 + 1 } else { pos.1 - 1 }));
    }
    if moves.is_empty() {
        return pos;
    }
    moves[rng.below(moves.len() as u64) as usize]
}

/// One turn for one robot. Returns a short description of what it did.
fn act(r: &mut Robot, hive: &mut Hive, ore: &mut [u32]) -> String {
    sense(r, hive, ore);

    if r.pos == BASE {
        r.energy = (r.energy + 8).min(MAX_ENERGY);
        if r.carrying > 0 {
            let n = r.carrying;
            hive.delivered += n;
            r.carrying = 0;
            claim(r, hive, None);
            return format!("delivers {n} ore to base");
        }
    }

    // Drop a target whose ore is gone, or that was already reached.
    if let Some(t) = r.target {
        if t != BASE && hive.cells[idx(t)].ore_known == 0 && r.carrying == 0 {
            claim(r, hive, None);
        }
    }
    if r.target == Some(r.pos) && r.pos != BASE && ore[idx(r.pos)] == 0 {
        claim(r, hive, None);
    }

    // Go home when full or when the energy only just covers the way back.
    let go_home = r.carrying > 0 || r.energy <= dist(r.pos, BASE) + 3;
    if go_home {
        if r.target != Some(BASE) {
            claim(r, hive, Some(BASE));
        }
    } else if r.target.is_none() {
        let next = choose(r, hive);
        claim(r, hive, next);
    }

    // Mine what is under the robot.
    if r.pos != BASE && ore[idx(r.pos)] > 0 && r.carrying < CAPACITY && !go_home {
        ore[idx(r.pos)] -= 1;
        hive.cells[idx(r.pos)].ore_known = ore[idx(r.pos)];
        r.carrying += 1;
        return format!("mines at {},{}", r.pos.0, r.pos.1);
    }

    match r.target {
        Some(t) if t != r.pos => {
            if r.energy <= 0 {
                return "out of energy".into();
            }
            r.pos = step_toward(r.pos, t, &mut r.rng);
            r.energy -= 1;
            if go_home {
                format!("returns to base, energy {}", r.energy)
            } else {
                format!("moves toward {},{}", t.0, t.1)
            }
        }
        _ => "waits".into(),
    }
}

fn known_map(hive: &Hive) -> String {
    let mut s = String::with_capacity(W * H);
    for (i, c) in hive.cells.iter().enumerate() {
        let p = (i % W, i / W);
        s.push(if p == BASE {
            'B'
        } else if !c.explored {
            ' '
        } else if c.ore_known > 0 {
            '*'
        } else {
            '.'
        });
    }
    s
}

fn frame_json(t: usize, robots: &[Robot], actions: &[String], hive: &Hive, ore: &[u32]) -> String {
    let explored = hive.cells.iter().filter(|c| c.explored).count();
    let ore_known = hive.cells.iter().filter(|c| c.ore_known > 0).count();
    let mut list = String::new();
    for (r, a) in robots.iter().zip(actions) {
        if !list.is_empty() {
            list.push(',');
        }
        let _ = write!(
            list,
            "{{\"name\":\"{}\",\"role\":\"{}\",\"x\":{},\"y\":{},\"carrying\":{},\"energy\":{},\"action\":\"{}\"}}",
            r.name,
            r.role.name(),
            r.pos.0,
            r.pos.1,
            r.carrying,
            r.energy,
            a
        );
    }
    format!(
        "{{\"tick\":{t},\"delivered\":{},\"explored\":{explored},\"ore_known\":{ore_known},\"ore_left\":{},\"known\":\"{}\",\"robots\":[{list}]}}",
        hive.delivered,
        ore.iter().sum::<u32>(),
        known_map(hive),
    )
}

fn arg(name: &str) -> Option<String> {
    let args: Vec<String> = env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1).cloned())
}

fn main() {
    let ticks: usize = arg("--ticks").and_then(|v| v.parse().ok()).unwrap_or(200);
    let seed: u64 = arg("--seed").and_then(|v| v.parse().ok()).unwrap_or(7);
    let out: PathBuf = arg("--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("runs/latest.js"));

    let mut world_rng = Rng::new(seed);
    let mut ore = place_ore(&mut world_rng);
    let total_ore: u32 = ore.iter().sum();

    let mut hive = Hive { cells: vec![Cell::default(); W * H], delivered: 0 };
    let mut counts = [0usize; 2];
    let mut robots: Vec<Robot> = ROLES
        .iter()
        .enumerate()
        .map(|(id, &role)| {
            let k = if role == Role::Scout { 0 } else { 1 };
            counts[k] += 1;
            Robot {
                id,
                name: format!("{}-{}", role.name(), counts[k]),
                role,
                pos: BASE,
                carrying: 0,
                energy: MAX_ENERGY,
                target: None,
                // Each robot's own stream, derived from the seed and its index.
                rng: Rng::new(seed ^ ((id as u64 + 1) << 32)),
            }
        })
        .collect();

    let mut frames = Vec::with_capacity(ticks);
    for t in 0..ticks {
        let n = robots.len();
        let mut actions = vec![String::new(); n];
        // Rotate who moves first, so no robot always has priority on a cell.
        for k in 0..n {
            let i = (k + t) % n;
            actions[i] = act(&mut robots[i], &mut hive, &mut ore);
        }
        frames.push(frame_json(t + 1, &robots, &actions, &hive, &ore));
    }

    let explored = hive.cells.iter().filter(|c| c.explored).count();
    let body = format!(
        "window.HIVE_DATA = {{\"width\":{W},\"height\":{H},\"base\":[{},{}],\"seed\":{seed},\"total_ore\":{total_ore},\"ticks\":[{}]}};\n",
        BASE.0,
        BASE.1,
        frames.join(",")
    );
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir).expect("create runs folder");
    }
    fs::write(&out, body).expect("write log");

    println!(
        "{ticks} ticks, seed {seed}: delivered {} of {total_ore} ore, explored {}% of the map",
        hive.delivered,
        explored * 100 / (W * H)
    );
    println!("log: {}", out.display());
}
