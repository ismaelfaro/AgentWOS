//! The ducks' bodies and the room they walk in.
//!
//! This is the layer MicroDuck keeps below its brain: the gait, the ToF
//! obstacle reflex, falls and the get-up watch. The numbers come from
//! pollen-robotics/microduck (`autod/src/obstacle.rs`, `docs/design/autonomous.md`,
//! `docs/design/simulation.md`, Apache-2.0). The physics is a flat 2D model,
//! not MuJoCo: no legs, no servos, no IMU.

use serde_json::{json, Value};

// Speeds are the pad's (`robotd_params::drive`): there is no faster autonomous mode.
pub const MAX_VX: f64 = 0.3;
pub const MAX_WZ: f64 = 1.5;
// The walking policy stands still at small commands; autod never asks for less.
pub const MIN_WALK_VX: f64 = 0.15;
const STOPS_BELOW_VX: f64 = 0.12;
// Every start from a standstill gets 0.6 s at full speed, or the gait does not start.
const START_BURST_S: f64 = 0.6;
// The gait walks slower than it is asked to (measured on the MuJoCo twin).
const GAIT_EFFICIENCY: f64 = 0.7;

// The ToF corridor, measured from the beak (`autod/src/obstacle.rs`).
pub const STOP_M: f64 = 0.15;
pub const REACT_M: f64 = 0.45;
const BEAK_M: f64 = 0.18;
const HALF_WIDTH_M: f64 = 0.12;
pub const MAX_RANGE_M: f64 = 1.5;

// Get-up watch (`autod/src/getup.rs`): up is fallen clear; still down after 5 s,
// cut the policy for 2 s and try once more; still down, go limp and ask for help.
const GETUP_TRY_S: f64 = 5.0;
const GETUP_CUT_S: f64 = 2.0;
// Chance per second that the standing policy works the duck upright.
const GETUP_RATE: f64 = 0.3;

// Energy, 0 to 1: units per second. Chasing burns twice a walk (one unit per 50 s).
const BURN_WALK: f64 = 1.0 / 100.0;
const BURN_ZOOMIES: f64 = 1.0 / 40.0;
const BURN_BALL: f64 = 1.0 / 70.0;
const REST_STAND: f64 = 1.0 / 300.0;
const REST_NAP: f64 = 1.0 / 40.0;

// A bump faster than this knocks a 25 cm duck over.
const FALL_SPEED: f64 = 0.18;
const BODY_R: f64 = 0.1;
const ARRIVE_M: f64 = 0.25;
const PIVOT_RAD: f64 = 0.7;
const SIT_S: f64 = 1.0;
const KICK_SPEED: f64 = 0.9;
const BALL_R: f64 = 0.05;
const BALL_FRICTION: f64 = 0.6;

pub const DT: f64 = 0.1;
pub const NOVELTY_CELL_M: f64 = 0.5;

/// What the brain can ask for: a subset of autod's states.
pub const STATES: [&str; 8] = ["chill", "look_around", "quack", "wander", "turn", "zoomies", "ball", "nap"];

pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    pub fn unit(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub enum Shape {
    Circle { x: f64, y: f64, r: f64 },
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },
}

pub struct Obstacle {
    pub name: &'static str,
    pub shape: Shape,
}

pub struct Room {
    pub w: f64,
    pub h: f64,
    pub obstacles: Vec<Obstacle>,
    pub cushion: (f64, f64),
}

impl Room {
    pub fn apartment() -> Self {
        let leg = |name, x, y| Obstacle { name, shape: Shape::Circle { x, y, r: 0.03 } };
        Room {
            w: 4.0,
            h: 3.0,
            obstacles: vec![
                Obstacle { name: "sofa", shape: Shape::Rect { x0: 0.0, y0: 2.3, x1: 1.6, y1: 3.0 } },
                Obstacle { name: "bookshelf", shape: Shape::Rect { x0: 3.6, y0: 0.0, x1: 4.0, y1: 1.2 } },
                leg("table leg", 2.2, 1.2),
                leg("table leg", 3.0, 1.2),
                leg("table leg", 2.2, 1.8),
                leg("table leg", 3.0, 1.8),
                Obstacle { name: "plant pot", shape: Shape::Circle { x: 0.6, y: 0.5, r: 0.15 } },
            ],
            cushion: (3.4, 2.6),
        }
    }

    pub fn json(&self) -> Value {
        let obstacles: Vec<Value> = self
            .obstacles
            .iter()
            .map(|o| match o.shape {
                Shape::Circle { x, y, r } => json!({ "name": o.name, "circle": [x, y, r] }),
                Shape::Rect { x0, y0, x1, y1 } => json!({ "name": o.name, "rect": [x0, y0, x1, y1] }),
            })
            .collect();
        json!({ "w": self.w, "h": self.h, "obstacles": obstacles, "cushion": [self.cushion.0, self.cushion.1] })
    }

    /// Distance from (x, y) to the nearest solid surface: walls and furniture.
    fn clearance(&self, x: f64, y: f64) -> (f64, &'static str) {
        let mut best = (x.min(y).min(self.w - x).min(self.h - y), "wall");
        for o in &self.obstacles {
            let d = match o.shape {
                Shape::Circle { x: cx, y: cy, r } => ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - r,
                Shape::Rect { x0, y0, x1, y1 } => {
                    let dx = (x0 - x).max(0.0).max(x - x1);
                    let dy = (y0 - y).max(0.0).max(y - y1);
                    (dx * dx + dy * dy).sqrt()
                }
            };
            if d < best.0 {
                best = (d, o.name);
            }
        }
        best
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Policy {
    Stand,
    Walk,
    Sit,
    Fallen,
    Limp,
}

impl Policy {
    pub fn name(self) -> &'static str {
        match self {
            Policy::Stand => "stand",
            Policy::Walk => "walk",
            Policy::Sit => "sit",
            Policy::Fallen => "fallen",
            Policy::Limp => "limp",
        }
    }
}

#[derive(Clone)]
pub struct Decision {
    pub state: String,
    pub goal: Option<(f64, f64)>,
    pub thought: String,
}

pub struct Duck {
    pub name: &'static str,
    pub x: f64,
    pub y: f64,
    pub heading: f64,
    pub energy: f64,
    pub policy: Policy,
    pub decision: Decision,
    pub rng: Rng,
    // Body-level timers.
    moving_s: f64,
    sit_s: f64,
    down_s: f64,
    getup_tries: u32,
    cut_s: f64,
    zoom_wz: f64,
    // What happened this round, for the brain and the dashboard.
    pub track: Vec<[f64; 2]>,
    pub events: Vec<String>,
    pub quacked: bool,
    pub walked_m: f64,
    pub novelty: Vec<f64>,
}

pub struct Ball {
    pub x: f64,
    pub y: f64,
    vx: f64,
    vy: f64,
    pub track: Vec<[f64; 2]>,
}

impl Ball {
    pub fn new(x: f64, y: f64) -> Self {
        Ball { x, y, vx: 0.0, vy: 0.0, track: Vec::new() }
    }
}

fn wrap(a: f64) -> f64 {
    let mut a = a;
    while a > std::f64::consts::PI {
        a -= std::f64::consts::TAU;
    }
    while a < -std::f64::consts::PI {
        a += std::f64::consts::TAU;
    }
    a
}

impl Duck {
    pub fn new(name: &'static str, x: f64, y: f64, heading: f64, seed: u64, room: &Room) -> Self {
        let cols = (room.w / NOVELTY_CELL_M).ceil() as usize;
        let rows = (room.h / NOVELTY_CELL_M).ceil() as usize;
        Duck {
            name,
            x,
            y,
            heading,
            energy: 0.8,
            policy: Policy::Stand,
            decision: Decision { state: "chill".into(), goal: None, thought: String::new() },
            rng: Rng::new(seed),
            moving_s: 0.0,
            sit_s: 0.0,
            down_s: 0.0,
            getup_tries: 0,
            cut_s: 0.0,
            zoom_wz: 0.0,
            track: Vec::new(),
            events: Vec::new(),
            quacked: false,
            walked_m: 0.0,
            novelty: vec![0.0; cols * rows],
        }
    }

    pub fn start_round(&mut self) {
        self.track = vec![[self.x, self.y]];
        self.events.clear();
        self.quacked = false;
        self.walked_m = 0.0;
    }

    /// The centre of the least-visited cell of the novelty grid, as autod's
    /// wander leans toward space the duck has not been in.
    pub fn least_visited(&self, room: &Room) -> (f64, f64) {
        let cols = (room.w / NOVELTY_CELL_M).ceil() as usize;
        let mut best = (f64::MAX, (room.w / 2.0, room.h / 2.0));
        for (i, dwell) in self.novelty.iter().enumerate() {
            let c = ((i % cols) as f64 + 0.5) * NOVELTY_CELL_M;
            let r = ((i / cols) as f64 + 0.5) * NOVELTY_CELL_M;
            if room.clearance(c, r).0 < 0.2 {
                continue;
            }
            let d = ((c - self.x).powi(2) + (r - self.y).powi(2)).sqrt();
            let score = dwell + d * 0.5;
            if score < best.0 {
                best = (score, (c, r));
            }
        }
        best.1
    }

    fn visit(&mut self, room: &Room) {
        let cols = (room.w / NOVELTY_CELL_M).ceil() as usize;
        let c = ((self.x / NOVELTY_CELL_M) as usize).min(cols - 1);
        let r = (self.y / NOVELTY_CELL_M) as usize;
        if let Some(v) = self.novelty.get_mut(r * cols + c) {
            *v += DT;
        }
    }
}

/// What the head ToF sees straight ahead: distance from the beak to the nearest
/// thing in the body's corridor, or None when the way is clear to `MAX_RANGE_M`.
pub fn tof_ahead(room: &Room, ducks: &[Duck], i: usize, heading_offset: f64) -> Option<(f64, &'static str)> {
    let d = &ducks[i];
    let h = d.heading + heading_offset;
    let (c, s) = (h.cos(), h.sin());
    let mut step = 0.0;
    while step <= MAX_RANGE_M {
        // Five rays 6 cm apart, each catching anything within 3 cm: a 6 cm chair leg cannot slip between.
        for lateral in [-HALF_WIDTH_M, -HALF_WIDTH_M / 2.0, 0.0, HALF_WIDTH_M / 2.0, HALF_WIDTH_M] {
            let px = d.x + c * (BEAK_M + step) - s * lateral;
            let py = d.y + s * (BEAK_M + step) + c * lateral;
            let (gap, what) = room.clearance(px, py);
            if gap <= 0.03 {
                return Some((step, what));
            }
            for (j, o) in ducks.iter().enumerate() {
                if j != i && ((px - o.x).powi(2) + (py - o.y).powi(2)).sqrt() < BODY_R {
                    return Some((step, "another duck"));
                }
            }
        }
        step += 0.03;
    }
    None
}

/// One body step for duck `i`: the brain's state through the gait and reflexes.
pub fn step(room: &Room, ducks: &mut [Duck], ball: &mut Ball, i: usize) {
    let ahead = tof_ahead(room, ducks, i, 0.0);
    let left = tof_ahead(room, ducks, i, 0.6).map_or(MAX_RANGE_M, |a| a.0);
    let right = tof_ahead(room, ducks, i, -0.6).map_or(MAX_RANGE_M, |a| a.0);
    let others: Vec<(f64, f64)> = ducks.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, o)| (o.x, o.y)).collect();
    let d = &mut ducks[i];

    // A duck that is down is robotd's and the get-up watch's, not the brain's.
    match d.policy {
        Policy::Limp => return,
        Policy::Fallen => {
            if d.cut_s > 0.0 {
                d.cut_s -= DT;
                if d.cut_s <= 0.0 {
                    d.events.push("policy given back for a second try".into());
                }
                return;
            }
            d.down_s += DT;
            if d.rng.unit() < GETUP_RATE * DT {
                d.policy = Policy::Stand;
                d.down_s = 0.0;
                d.getup_tries = 0;
                d.events.push("got back up".into());
            } else if d.down_s >= GETUP_TRY_S {
                d.down_s = 0.0;
                d.getup_tries += 1;
                if d.getup_tries >= 2 {
                    d.policy = Policy::Limp;
                    d.events.push("still down after two tries: torque off, questioning quack, needs a hand".into());
                } else {
                    d.cut_s = GETUP_CUT_S;
                    d.events.push("still down after 5 s: policy off for 2 s to settle".into());
                }
            }
            return;
        }
        _ => {}
    }

    let state = d.decision.state.clone();

    // Sitting is a policy of its own: sit_toggle down for a nap, up for anything else.
    if state == "nap" {
        if d.policy != Policy::Sit {
            d.sit_s += DT;
            if d.sit_s >= SIT_S {
                d.policy = Policy::Sit;
                d.sit_s = 0.0;
                d.events.push("sat down for a nap".into());
            }
        } else {
            d.energy = (d.energy + REST_NAP * DT).min(1.0);
        }
        return;
    }
    if d.policy == Policy::Sit {
        d.sit_s += DT;
        if d.sit_s >= SIT_S {
            d.policy = Policy::Stand;
            d.sit_s = 0.0;
            d.events.push("stood up from the nap".into());
        }
        return;
    }

    // The brain's intent as a twist.
    let (mut vx, mut wz) = (0.0, 0.0);
    let target = match state.as_str() {
        "ball" => Some((ball.x, ball.y)),
        "wander" | "turn" => d.decision.goal,
        _ => None,
    };
    match state.as_str() {
        "wander" | "ball" => {
            if let Some((gx, gy)) = target {
                let dist = ((gx - d.x).powi(2) + (gy - d.y).powi(2)).sqrt();
                let err = wrap((gy - d.y).atan2(gx - d.x) - d.heading);
                let arrive = if state == "ball" { 0.0 } else { ARRIVE_M };
                if dist > arrive {
                    wz = (2.0 * err).clamp(-MAX_WZ, MAX_WZ);
                    if err.abs() < PIVOT_RAD {
                        vx = if dist < 0.4 { MIN_WALK_VX } else { MAX_VX };
                    }
                } else if !d.events.iter().any(|e| e == "arrived at its goal") {
                    d.events.push("arrived at its goal".into());
                }
            }
        }
        "turn" => {
            let (gx, gy) = target.unwrap_or_else(|| (d.x - d.heading.cos(), d.y - d.heading.sin()));
            let err = wrap((gy - d.y).atan2(gx - d.x) - d.heading);
            if err.abs() > 0.15 {
                wz = MAX_WZ * err.signum();
            }
        }
        "zoomies" => {
            if d.rng.unit() < 0.2 {
                d.zoom_wz = (d.rng.unit() * 2.0 - 1.0) * MAX_WZ;
            }
            vx = MAX_VX;
            wz = d.zoom_wz;
        }
        _ => {}
    }

    // The ToF reflex, as autod's gate: slow and steer under REACT_M, spin away under STOP_M.
    let steer_away = if left >= right { MAX_WZ } else { -MAX_WZ };
    let reacting = ahead.is_some_and(|(dist, _)| dist < REACT_M);
    if vx > 0.0 {
        if let Some((dist, what)) = ahead {
            if dist < STOP_M {
                vx = 0.0;
                wz = steer_away;
                if !d.events.iter().any(|e| e.starts_with("ToF stop")) {
                    d.events.push(format!("ToF stop: {what} at {:.2} m, spinning away", dist));
                }
            } else if dist < REACT_M {
                vx = (vx * 0.5).max(MIN_WALK_VX);
                wz = (wz + steer_away * 0.5).clamp(-MAX_WZ, MAX_WZ);
            }
        }
    }

    // The gait: a start from a standstill needs a burst; small commands stand.
    if vx > 0.0 {
        if d.policy != Policy::Walk {
            d.policy = Policy::Walk;
            d.moving_s = 0.0;
        }
        // No burst into something the ToF already sees.
        if d.moving_s < START_BURST_S && !reacting {
            vx = MAX_VX;
        } else if vx < STOPS_BELOW_VX {
            vx = 0.0;
        }
        d.moving_s += DT;
    } else if d.policy == Policy::Walk {
        d.policy = Policy::Stand;
    }

    let speed = vx * GAIT_EFFICIENCY;
    d.heading = wrap(d.heading + wz * DT);
    let nx = d.x + d.heading.cos() * speed * DT;
    let ny = d.y + d.heading.sin() * speed * DT;

    // Contact: walls, furniture, other ducks. A fast bump is a fall.
    let (gap, what) = room.clearance(nx, ny);
    let duck_hit = others.iter().any(|(ox, oy)| ((nx - ox).powi(2) + (ny - oy).powi(2)).sqrt() < 2.0 * BODY_R);
    if gap < BODY_R || duck_hit {
        let into = if duck_hit { "another duck" } else { what };
        if speed > FALL_SPEED {
            d.policy = Policy::Fallen;
            d.down_s = 0.0;
            d.events.push(format!("fell after bumping into {into}"));
        } else if !d.events.iter().any(|e| e.starts_with("bumped")) {
            d.events.push(format!("bumped into {into}"));
        }
    } else {
        d.walked_m += ((nx - d.x).powi(2) + (ny - d.y).powi(2)).sqrt();
        d.x = nx;
        d.y = ny;
    }

    // Kicking: the beak reaches the ball.
    if state == "ball" {
        let bx = d.x + d.heading.cos() * BEAK_M;
        let by = d.y + d.heading.sin() * BEAK_M;
        if ((bx - ball.x).powi(2) + (by - ball.y).powi(2)).sqrt() < BALL_R + 0.05 {
            ball.vx = d.heading.cos() * KICK_SPEED;
            ball.vy = d.heading.sin() * KICK_SPEED;
            d.events.push("kicked the ball".into());
        }
    }

    let burn = match state.as_str() {
        "zoomies" => BURN_ZOOMIES,
        "ball" => BURN_BALL,
        _ if vx > 0.0 || wz != 0.0 => BURN_WALK,
        _ => -REST_STAND,
    };
    d.energy = (d.energy - burn * DT).clamp(0.0, 1.0);
    if state == "quack" && !d.quacked {
        d.quacked = true;
        d.events.push("quacked".into());
    }
}

pub fn step_ball(room: &Room, ball: &mut Ball) {
    let nx = ball.x + ball.vx * DT;
    let ny = ball.y + ball.vy * DT;
    if room.clearance(nx, ny).0 < BALL_R {
        // Bounce off whatever it hit, losing half its speed.
        ball.vx = -ball.vx * 0.5;
        ball.vy = -ball.vy * 0.5;
    } else {
        ball.x = nx;
        ball.y = ny;
    }
    let decay = (1.0 - BALL_FRICTION * DT).max(0.0);
    ball.vx *= decay;
    ball.vy *= decay;
}

pub fn after_step(room: &Room, duck: &mut Duck, sample: bool) {
    duck.visit(room);
    if sample {
        duck.track.push([duck.x, duck.y]);
    }
}
