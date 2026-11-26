//! One match of duck soccer: two teams of three MicroDucks, a ball, two goals.
//!
//! Each duck plays by a small policy of eight numbers (its genome) and talks to
//! its team with sounds whose meaning it learns while it plays. The physics is a
//! flat 2D model; the numbers are chosen to feel like small robots on a mat, not
//! to match MicroDuck's real gait.

use serde_json::{json, Value};

pub const W: f64 = 10.0;
pub const H: f64 = 6.0;
pub const GOAL_W: f64 = 2.0;
pub const DT: f64 = 0.1;
/// Ducks per team.
pub const N: usize = 3;
/// Numbers in a duck's policy: eight for how it plays, three for how much it
/// heeds each call ("open", "mine", "back").
pub const G: usize = 11;

/// What a call can mean: "I am free, pass to me", "the ball is mine, stay off",
/// "they are attacking, come back".
pub const MEANINGS: [&str; 3] = ["open", "mine", "back"];
pub const SOUNDS: [&str; 4] = ["quack", "chirp", "honk", "purr"];
/// A duck's weight for every (meaning, sound) pair.
pub type Lexicon = [[f64; 4]; 3];

const VMAX: f64 = 1.6;
/// How far a duck sees. A MicroDuck looks through a camera and a short-range depth
/// sensor in its head, so it does not see the whole pitch; what it cannot see, a
/// teammate can tell it.
pub const VIEW_R: f64 = 3.5;
/// What a duck remembers of the ball it no longer sees, at most this long.
const MEMORY_S: f64 = 3.0;
const KICK_RANGE: f64 = 0.4;
const DUCK_R: f64 = 0.2;
const FRICTION: f64 = 0.9;
const KICK_COOLDOWN_S: f64 = 0.4;
const SPEAK_GAP_S: f64 = 2.0;
const WINDOW_S: f64 = 3.0;
/// A call is not always heard: the room is noisy.
const HEAR_LOSS: f64 = 0.08;
const MIN_W: f64 = 0.05;
const MAX_W: f64 = 50.0;

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
    pub fn gauss(&mut self) -> f64 {
        let (u1, u2) = (self.unit().max(1e-12), self.unit());
        (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }
}

/// A team: three policies, three vocabularies and how fast the team learns words.
#[derive(Clone)]
pub struct Team {
    pub genome: [[f64; G]; N],
    pub lex: [Lexicon; N],
    pub reward: f64,
    pub penalty: f64,
    /// How readily the team makes each call, 0 to 1. Evolution can silence a word that hurts.
    pub use_call: [f64; 3],
}

impl Team {
    pub fn random(rng: &mut Rng) -> Team {
        let mut genome = [[0.0; G]; N];
        for g in genome.iter_mut() {
            for v in g.iter_mut() {
                *v = rng.unit();
            }
        }
        let use_call = [rng.unit(), rng.unit(), rng.unit()];
        Team { genome, lex: Self::fresh_lexicons(rng), reward: 1.0, penalty: 0.5, use_call }
    }

    /// Each duck starts with its own accent: a small random spread on every pair.
    pub fn fresh_lexicons(rng: &mut Rng) -> [Lexicon; N] {
        let mut lex = [[[0.0; 4]; 3]; N];
        for l in lex.iter_mut() {
            for m in l.iter_mut() {
                for w in m.iter_mut() {
                    *w = 1.0 + rng.unit() * 0.6;
                }
            }
        }
        lex
    }

    /// The opponent every generation is measured against: three ducks that all run
    /// at the ball and shoot from mid-range. They never pass and never talk.
    pub fn chasers() -> Team {
        let one = |width: f64| [0.45, width, 1.0, 0.55, 0.0, 0.4, 0.4, 0.7, 0.0, 0.0, 0.0];
        Team { genome: [one(0.2), one(0.5), one(0.8)], lex: [[[1.0; 4]; 3]; N], reward: 1.0, penalty: 0.5, use_call: [0.0; 3] }
    }
}

pub struct Options {
    /// Whether each team may talk.
    pub comms: [bool; 2],
    /// Keep frames and events for a replay.
    pub record: bool,
    /// Which calls the talking teams may make, by meaning (to measure each word).
    pub calls: [bool; 3],
    pub seconds: f64,
}

#[derive(Default, Clone, Copy)]
pub struct Talked {
    pub said: u32,
    /// The listener's action worked.
    pub ok: u32,
    pub missed: u32,
    /// The listener took the call to mean what the speaker meant.
    pub correct: u32,
}

pub struct Outcome {
    pub score: [u32; 2],
    pub lex: [[Lexicon; N]; 2],
    pub talk: [Talked; 2],
    pub passes: [u32; 2],
    pub frames: Vec<Vec<f64>>,
    pub events: Vec<Value>,
}

#[derive(Clone, Copy)]
struct Player {
    team: usize,
    idx: usize,
    x: f64,
    y: f64,
    cooldown: f64,
    gap: f64,
    no_chase_until: f64,
    back_until: f64,
    /// A teammate who called "open" to this duck, and until when it counts.
    call: Option<(usize, f64)>,
    /// Where this duck last saw the ball, whether the other team had it, and when.
    kb: (f64, f64),
    k_theirs: bool,
    k_at: f64,
}

struct Pending {
    team: usize,
    listener: usize,
    speaker: usize,
    meant: usize,
    believed: usize,
    sound: usize,
    since: f64,
    start_lx: f64,
    start_d: f64,
}

struct Match<'a> {
    teams: [Team; 2],
    opt: &'a Options,
    p: Vec<Player>,
    bx: f64,
    by: f64,
    bvx: f64,
    bvy: f64,
    last_touch: Option<(usize, usize)>,
    touch_t: f64,
    t: f64,
    score: [u32; 2],
    rng: Rng,
    pending: Vec<Pending>,
    talk: [Talked; 2],
    passes: [u32; 2],
    /// A pass in flight: (team, from, to).
    pass: Option<(usize, usize, usize)>,
    /// The last pass each duck made: (to, when). A caller sees whether it was to them.
    passed: Vec<Option<(usize, f64)>>,
    /// Which ducks are running at the ball this tick: what a teammate can see.
    chase: Vec<bool>,
    frames: Vec<Vec<f64>>,
    events: Vec<Value>,
    /// Tests fix the kick order.
    order: Option<usize>,
}

/// x in a team's own frame: every team attacks toward local x = W.
fn local(team: usize, x: f64) -> f64 {
    if team == 0 {
        x
    } else {
        W - x
    }
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// Distance from point c to the segment a-b.
fn seg_dist(a: (f64, f64), b: (f64, f64), c: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    if len2 == 0.0 {
        return dist(a, c);
    }
    let k = (((c.0 - a.0) * dx + (c.1 - a.1) * dy) / len2).clamp(0.0, 1.0);
    dist((a.0 + k * dx, a.1 + k * dy), c)
}

fn argmax(v: &[f64]) -> usize {
    let mut best = 0;
    for (k, x) in v.iter().enumerate() {
        if *x > v[best] {
            best = k;
        }
    }
    best
}

impl<'a> Match<'a> {
    fn gene(&self, i: usize, k: usize) -> f64 {
        let pl = &self.p[i];
        self.teams[pl.team].genome[pl.idx][k]
    }

    /// Where a duck stands when it is not chasing, in world coordinates.
    fn home(&self, i: usize) -> (f64, f64) {
        let pl = self.p[i];
        let base = 1.2 + 6.0 * self.gene(i, 0);
        let width = 0.6 + 4.8 * self.gene(i, 1);
        let follow = 0.8 * self.gene(i, 5);
        let defend = 1.0 + 3.5 * self.gene(i, 6);
        let fresh = self.t - pl.k_at < MEMORY_S;
        let ball_l = local(pl.team, pl.kb.0);
        let mut hx = base + follow * (ball_l - W / 2.0);
        if fresh && pl.k_theirs && ball_l < W / 2.0 {
            hx = hx.min(defend);
        }
        if self.t < pl.back_until {
            hx = hx.min(2.0);
        }
        let hy = width + 0.3 * (pl.kb.1 - width);
        (local(pl.team, hx.clamp(0.5, W - 0.5)), hy.clamp(0.3, H - 0.3))
    }

    /// Kick-off: the team that kicks off may stand by the ball, the other waits 2 m back.
    fn reset(&mut self, kicks_off: usize) {
        for i in 0..self.p.len() {
            let pl = self.p[i];
            let base = 1.2 + 6.0 * self.gene(i, 0);
            let width = 0.6 + 4.8 * self.gene(i, 1);
            let lx = base.min(W / 2.0 - if pl.team == kicks_off { 0.6 } else { 2.0 });
            self.p[i].x = local(pl.team, lx);
            self.p[i].y = width;
            self.p[i].cooldown = 0.0;
            self.p[i].call = None;
        }
        self.bx = W / 2.0;
        self.by = H / 2.0;
        self.bvx = 0.0;
        self.bvy = 0.0;
        self.last_touch = None;
        self.pass = None;
    }

    /// Turns a direction by a random angle with the given spread (radians).
    fn wobble(&mut self, team: usize, dx: f64, dy: f64, spread: f64) -> (f64, f64) {
        // Mirrored for the team attacking left, so both halves play by the same rules.
        let a = spread * self.rng.gauss() * if team == 0 { 1.0 } else { -1.0 };
        (dx * a.cos() - dy * a.sin(), dx * a.sin() + dy * a.cos())
    }

    fn sees(&self, i: usize, at: (f64, f64)) -> bool {
        dist((self.p[i].x, self.p[i].y), at) <= VIEW_R
    }

    /// Each tick a duck updates what it knows of the ball, if it can see it.
    fn look(&mut self) {
        for i in 0..self.p.len() {
            if self.sees(i, (self.bx, self.by)) {
                let team = self.p[i].team;
                self.p[i].kb = (self.bx, self.by);
                self.p[i].k_theirs = self.last_touch.is_some_and(|(t, _)| t != team);
                self.p[i].k_at = self.t;
            }
        }
    }

    fn has_ball(&self, i: usize) -> bool {
        let pl = &self.p[i];
        pl.cooldown <= 0.0 && dist((pl.x, pl.y), (self.bx, self.by)) < KICK_RANGE
    }

    /// The nearest opponent this duck can see; one it cannot see is not a worry to it.
    fn nearest_opponent(&self, i: usize) -> f64 {
        let pl = self.p[i];
        self.p
            .iter()
            .filter(|o| o.team != pl.team && dist((o.x, o.y), (pl.x, pl.y)) <= VIEW_R)
            .map(|o| dist((o.x, o.y), (pl.x, pl.y)))
            .fold(VIEW_R, f64::min)
    }

    /// One decision: where to go, and a kick (direction, power) if the duck has the ball.
    fn decide(&mut self, i: usize) -> ((f64, f64), Option<(f64, f64, f64)>) {
        let pl = self.p[i];
        let me = (pl.x, pl.y);
        let ball = (self.bx, self.by);
        if self.has_ball(i) {
            let goal = (local(pl.team, W), H / 2.0);
            let shoot_range = 1.5 + 4.0 * self.gene(i, 3);
            if dist(me, goal) < shoot_range {
                // Aim at the side of the goal away from the nearest defender.
                let keeper_y = self
                    .p
                    .iter()
                    .filter(|o| o.team != pl.team && dist((o.x, o.y), me) <= VIEW_R)
                    .min_by(|a, b| dist((a.x, a.y), goal).partial_cmp(&dist((b.x, b.y), goal)).unwrap())
                    .map_or(H / 2.0, |o| o.y);
                let side = if keeper_y > H / 2.0 { -1.0 } else { 1.0 };
                let aim_y = H / 2.0 + side * (GOAL_W / 2.0 - 0.3) + (self.rng.unit() - 0.5) * 0.4;
                let aim = (local(pl.team, W + 0.3), aim_y);
                let power = 3.0 + 3.0 * self.gene(i, 7);
                // The farther and harder the shot, the wider it may go.
                let spread = 0.05 + 0.03 * dist(me, goal) + 0.02 * (power - 3.0);
                let (dx, dy) = self.wobble(pl.team, aim.0 - me.0, aim.1 - me.1, spread);
                return (ball, Some((dx, dy, power)));
            }
            // A pass, to whoever is ahead, free and reachable; a call to us counts for a lot.
            let bias = self.gene(i, 4);
            let mut best: Option<(f64, usize)> = None;
            for j in 0..self.p.len() {
                let mate = self.p[j];
                if j == i || mate.team != pl.team {
                    continue;
                }
                let d = dist(me, (mate.x, mate.y));
                if !(1.0..=7.0).contains(&d) {
                    continue;
                }
                let called = pl.call.is_some_and(|(c, until)| c == mate.idx && self.t < until);
                if d > VIEW_R && !called {
                    continue;
                }
                let lane_clear = self
                    .p
                    .iter()
                    .filter(|o| o.team != pl.team && dist((o.x, o.y), me) <= VIEW_R)
                    .all(|o| seg_dist(me, (mate.x, mate.y), (o.x, o.y)) > 0.5);
                if !lane_clear {
                    continue;
                }
                let ahead = local(pl.team, mate.x) - local(pl.team, pl.x);
                let heed = 4.0 * self.gene(i, 8);
                let score = ahead * 0.6 + self.nearest_opponent(j).min(2.5) - 0.15 * d + if called { heed } else { 0.0 };
                if best.is_none_or(|(s, _)| score > s) {
                    best = Some((score, j));
                }
            }
            if let Some((score, j)) = best {
                if score >= 2.6 * (1.0 - bias) + 0.4 {
                    let mate = self.p[j];
                    let d = dist(me, (mate.x, mate.y));
                    self.pass = Some((pl.team, pl.idx, mate.idx));
                    self.passed[i] = Some((mate.idx, self.t));
                    let (dx, dy) = self.wobble(pl.team, mate.x - me.0, mate.y - me.1, 0.04 + 0.01 * d);
                    return (ball, Some((dx, dy, (1.3 * d + 1.0).clamp(2.5, 6.0))));
                }
            }
            // Dribble: a short kick toward goal, drifting to the middle.
            let ahead = (local(pl.team, local(pl.team, pl.x) + 1.2), pl.y + 0.3 * (H / 2.0 - pl.y));
            return (ball, Some((ahead.0 - me.0, ahead.1 - me.1, 2.2)));
        }

        // Without the ball: chase it if this duck is the closest teammate, or it is near.
        let chase_r = 0.6 + 4.4 * self.gene(i, 2);
        let my_d = dist(me, ball);
        let visible = my_d <= VIEW_R;
        let closest = self
            .p
            .iter()
            .enumerate()
            .filter(|(j, o)| o.team == pl.team && self.t >= o.no_chase_until && *j != i && dist((o.x, o.y), me) <= VIEW_R)
            .all(|(_, o)| dist((o.x, o.y), ball) >= my_d);
        let chase = visible && self.t >= pl.no_chase_until && (closest || my_d < chase_r);
        if chase {
            return ((self.bx + self.bvx * 0.3, self.by + self.bvy * 0.3), None);
        }
        if self.t < pl.no_chase_until {
            // Told the ball is a teammate's: cover, 1.5 m behind the ball on our side and a step wide.
            let side = if pl.y > self.by { 1.0 } else { -1.0 };
            let cover_x = local(pl.team, (local(pl.team, self.bx) - 1.5).max(0.5));
            return ((cover_x, (self.by + side * 1.0).clamp(0.3, H - 0.3)), None);
        }
        (self.home(i), None)
    }

    fn step(&mut self) {
        self.look();
        let n = self.p.len();
        let plans: Vec<_> = (0..n).map(|i| self.decide(i)).collect();
        let chasing: Vec<bool> = (0..n).map(|i| plans[i].1.is_none() && dist(plans[i].0, (self.bx, self.by)) < 0.6).collect();
        self.chase = chasing.clone();

        // Move.
        for (i, (target, _)) in plans.iter().enumerate() {
            let pl = &mut self.p[i];
            let (dx, dy) = (target.0 - pl.x, target.1 - pl.y);
            let d = (dx * dx + dy * dy).sqrt();
            if d > 0.05 {
                let s = (VMAX * DT).min(d) / d;
                pl.x = (pl.x + dx * s).clamp(0.1, W - 0.1);
                pl.y = (pl.y + dy * s).clamp(0.1, H - 0.1);
            }
            pl.cooldown -= DT;
            pl.gap -= DT;
        }
        // Ducks do not walk through each other. Pushes are worked out from one
        // snapshot: resolving pairs in turn favoured the ducks listed last.
        let mut shove = vec![(0.0, 0.0); n];
        for a in 0..n {
            for b in a + 1..n {
                let d = dist((self.p[a].x, self.p[a].y), (self.p[b].x, self.p[b].y));
                if d < 2.0 * DUCK_R && d > 1e-6 {
                    let push = (2.0 * DUCK_R - d) / 2.0;
                    let (ux, uy) = ((self.p[b].x - self.p[a].x) / d, (self.p[b].y - self.p[a].y) / d);
                    shove[a].0 -= ux * push;
                    shove[a].1 -= uy * push;
                    shove[b].0 += ux * push;
                    shove[b].1 += uy * push;
                }
            }
        }
        for (pl, (sx, sy)) in self.p.iter_mut().zip(shove) {
            pl.x += sx;
            pl.y += sy;
        }

        // Kick, in a rotating order so no duck always reaches the ball first.
        let start = self.order.unwrap_or_else(|| (self.rng.unit() * n as f64) as usize % n);
        for k in 0..n {
            let i = (k + start) % n;
            if let Some((dx, dy, power)) = plans[i].1 {
                if !self.has_ball(i) {
                    continue;
                }
                let d = (dx * dx + dy * dy).sqrt().max(1e-6);
                self.bvx = dx / d * power;
                self.bvy = dy / d * power;
                self.p[i].cooldown = KICK_COOLDOWN_S;
                self.touch(i);
                break;
            }
        }

        // The ball.
        self.bx += self.bvx * DT;
        self.by += self.bvy * DT;
        let decay = (-FRICTION * DT).exp();
        self.bvx *= decay;
        self.bvy *= decay;
        if self.by < 0.0 || self.by > H {
            self.by = self.by.clamp(0.0, H);
            self.bvy = -self.bvy * 0.8;
        }
        let in_mouth = (self.by - H / 2.0).abs() < GOAL_W / 2.0;
        if self.bx < 0.0 || self.bx > W {
            if in_mouth {
                let scorer = if self.bx > W { 0 } else { 1 };
                self.goal(scorer);
                return;
            }
            self.bx = self.bx.clamp(0.0, W);
            self.bvx = -self.bvx * 0.6;
        }
        // A duck the ball runs into: a slow ball is nudged, a fast one is blocked.
        for k in 0..n {
            let i = (k + start) % n;
            let pl = self.p[i];
            let d = dist((pl.x, pl.y), (self.bx, self.by));
            if d >= 0.25 || d < 1e-6 {
                continue;
            }
            let just_kicked = self.last_touch == Some((pl.team, pl.idx)) && self.t - self.touch_t < 0.3;
            if just_kicked {
                continue;
            }
            let speed = self.bvx.hypot(self.bvy);
            let (ux, uy) = ((self.bx - pl.x) / d, (self.by - pl.y) / d);
            if speed < 0.6 {
                self.bvx += ux * 0.8;
                self.bvy += uy * 0.8;
            } else {
                // The duck's body stops most of it and the ball bounces off.
                self.bvx = ux * speed * 0.3;
                self.bvy = uy * speed * 0.3;
            }
            self.touch(i);
            break;
        }

        if self.opt.comms[0] || self.opt.comms[1] {
            self.talk_triggers(&chasing);
            self.resolve();
        }
        self.t += DT;
        if self.opt.record && ((self.t / DT).round() as usize) % 2 == 0 {
            let mut f = vec![(self.t * 10.0).round() / 10.0, self.bx, self.by];
            for pl in &self.p {
                f.push((pl.x * 100.0).round() / 100.0);
                f.push((pl.y * 100.0).round() / 100.0);
            }
            f[1] = (f[1] * 100.0).round() / 100.0;
            f[2] = (f[2] * 100.0).round() / 100.0;
            self.frames.push(f);
        }
    }

    fn touch(&mut self, i: usize) {
        let pl = self.p[i];
        if let Some((team, from, to)) = self.pass {
            if team == pl.team && pl.idx == to && from != to {
                self.passes[team] += 1;
            }
            if team != pl.team || pl.idx != from {
                self.pass = None;
            }
        }
        self.last_touch = Some((pl.team, pl.idx));
        self.touch_t = self.t;
    }

    fn goal(&mut self, scorer: usize) {
        self.score[scorer] += 1;
        if self.opt.record {
            self.events.push(json!({ "t": self.t, "event": "goal", "team": scorer, "score": self.score }));
        }
        self.reset(1 - scorer);
    }

    fn idx_of(&self, team: usize, idx: usize) -> usize {
        team * N + idx
    }

    fn talk_triggers(&mut self, chasing: &[bool]) {
        let n = self.p.len();
        for i in 0..n {
            let pl = self.p[i];
            if !self.opt.comms[pl.team] || pl.gap > 0.0 {
                continue;
            }
            let mates: Vec<usize> = (0..n).filter(|&j| j != i && self.p[j].team == pl.team).collect();
            // What this duck can see decides what it says.
            let sees_ball = self.sees(i, (self.bx, self.by));
            let carrier = mates.iter().copied().find(|&j| sees_ball && dist((self.p[j].x, self.p[j].y), (self.bx, self.by)) < KICK_RANGE);
            let ours = sees_ball && self.last_touch.is_some_and(|(t, _)| t == pl.team);
            let theirs = sees_ball && self.last_touch.is_some_and(|(t, _)| t != pl.team);
            let meaning = if let (Some(c), true) = (carrier, ours) {
                // Free and not behind the ball: ask for it.
                let free = self.nearest_opponent(i) > 1.5;
                let level = local(pl.team, pl.x) > local(pl.team, self.p[c].x) - 1.0;
                (free && level && !chasing[i]).then_some(0)
            } else if chasing[i] && mates.iter().any(|&j| chasing[j] && self.sees(i, (self.p[j].x, self.p[j].y))) {
                Some(1)
            } else if theirs && local(pl.team, self.bx) < 4.0 && mates.iter().all(|&j| local(pl.team, self.p[j].x) > local(pl.team, pl.x)) {
                // The last duck back sees the attack coming; the ones ahead may not.
                Some(2)
            } else {
                None
            };
            if let Some(m) = meaning.filter(|&m| self.opt.calls[m]) {
                if self.rng.unit() < self.teams[pl.team].use_call[m] {
                    self.speak(i, m);
                } else {
                    // Kept quiet this time; wait as if it had spoken.
                    self.p[i].gap = SPEAK_GAP_S;
                }
            }
        }
    }

    fn speak(&mut self, i: usize, meant: usize) {
        let pl = self.p[i];
        let w = self.teams[pl.team].lex[pl.idx][meant];
        let total: f64 = w.iter().sum();
        let mut roll = self.rng.unit() * total;
        let mut sound = w.len() - 1;
        for (s, x) in w.iter().enumerate() {
            if roll < *x {
                sound = s;
                break;
            }
            roll -= x;
        }
        self.p[i].gap = SPEAK_GAP_S;
        self.talk[pl.team].said += 1;
        if self.opt.record {
            self.events.push(json!({ "t": self.t, "event": "said", "team": pl.team, "duck": pl.idx, "sound": SOUNDS[sound], "meant": MEANINGS[meant] }));
        }
        // "Open" asks the duck with the ball for a pass, so it is meant for that duck alone.
        let carrier = (0..self.p.len()).find(|&j| j != i && self.p[j].team == pl.team && dist((self.p[j].x, self.p[j].y), (self.bx, self.by)) < KICK_RANGE);
        for j in 0..self.p.len() {
            let mate = self.p[j];
            if j == i || mate.team != pl.team || self.rng.unit() < HEAR_LOSS {
                continue;
            }
            if meant == 0 && Some(j) != carrier {
                continue;
            }
            // "Mine" is for a teammate who is also running at the ball.
            if meant == 1 && !self.chase[j] {
                continue;
            }
            let lex = &self.teams[pl.team].lex[mate.idx];
            let believed = argmax(&lex.iter().map(|m| m[sound]).collect::<Vec<_>>());
            // The listener acts on what it believes it heard.
            // How much the listener heeds a call is part of its genome.
            match believed {
                0 => self.p[j].call = Some((pl.idx, self.t + WINDOW_S)),
                1 => self.p[j].no_chase_until = self.t + 2.0 * self.gene(j, 9),
                _ => {
                    self.p[j].back_until = self.t + 3.0 * self.gene(j, 10);
                    self.p[j].k_theirs = true;
                    self.p[j].k_at = self.t;
                }
            }
            // A previous call this listener was still acting on is settled first.
            if let Some(k) = self.pending.iter().position(|p| p.team == pl.team && p.listener == mate.idx) {
                let old = self.pending.swap_remove(k);
                self.settle(old, false);
            }
            self.pending.push(Pending {
                team: pl.team,
                listener: mate.idx,
                speaker: pl.idx,
                meant,
                believed,
                sound,
                since: self.t,
                start_lx: local(mate.team, mate.x),
                start_d: dist((mate.x, mate.y), (self.bx, self.by)),
            });
        }
    }

    /// Did the listener do what the speaker asked? The speaker can see it: a pass to
    /// it, a teammate leaving the ball, a teammate running back. That is the only
    /// feedback; nobody can see what another duck understood.
    fn resolve(&mut self) {
        let mut k = 0;
        while k < self.pending.len() {
            let p = &self.pending[k];
            let age = self.t - p.since;
            let li = self.idx_of(p.team, p.listener);
            let listener = self.p[li];
            let (ok, done) = match p.meant {
                0 => {
                    let passed = self.passed[li].is_some_and(|(to, when)| to == p.speaker && when >= p.since);
                    (passed, passed || age > WINDOW_S)
                }
                1 => {
                    // It was running at the ball; half a second later it has stopped.
                    let _ = p.start_d;
                    (!self.chase[li], age >= 0.5)
                }
                _ => {
                    // It came all the way back to our own third, and not after the ball.
                    let back = local(p.team, listener.x) < 2.3 && !self.chase[li] && p.start_lx > 2.3;
                    (back, back || age > WINDOW_S)
                }
            };
            if done {
                let p = self.pending.swap_remove(k);
                self.settle(p, ok);
            } else {
                k += 1;
            }
        }
    }

    fn settle(&mut self, p: Pending, ok: bool) {
        let team = &mut self.teams[p.team];
        let (reward, penalty) = (team.reward, team.penalty);
        let bump = |w: &mut f64, by: f64| *w = (*w + by).clamp(MIN_W, MAX_W);
        // Both sides learn from what the speaker saw: the listener about its reading
        // of the sound, the speaker about its choice of sound.
        if ok {
            bump(&mut team.lex[p.listener][p.believed][p.sound], reward);
            bump(&mut team.lex[p.speaker][p.meant][p.sound], reward);
            self.talk[p.team].ok += 1;
        } else {
            bump(&mut team.lex[p.listener][p.believed][p.sound], -penalty);
            bump(&mut team.lex[p.speaker][p.meant][p.sound], -penalty * 0.5);
            self.talk[p.team].missed += 1;
        }
        if p.believed == p.meant {
            self.talk[p.team].correct += 1;
        }
        if self.opt.record {
            self.events.push(json!({
                "t": self.t, "event": if ok { "understood" } else { "missed" }, "team": p.team,
                "listener": p.listener, "speaker": p.speaker, "sound": SOUNDS[p.sound],
                "meant": MEANINGS[p.meant], "believed": MEANINGS[p.believed],
            }));
        }
    }
}

/// Plays one match. Team 0 attacks to the right. Each team learns words during
/// the match; the outcome carries its vocabularies at the final whistle.
pub fn play(teams: [&Team; 2], seed: u64, opt: &Options) -> Outcome {
    let mut p = Vec::with_capacity(2 * N);
    for team in 0..2 {
        for idx in 0..N {
            p.push(Player { team, idx, x: 0.0, y: 0.0, cooldown: 0.0, gap: 0.0, no_chase_until: 0.0, back_until: 0.0, call: None, kb: (W / 2.0, H / 2.0), k_theirs: false, k_at: 0.0 });
        }
    }
    let mut m = Match {
        teams: [teams[0].clone(), teams[1].clone()],
        opt,
        p,
        bx: 0.0,
        by: 0.0,
        bvx: 0.0,
        bvy: 0.0,
        last_touch: None,
        touch_t: 0.0,
        t: 0.0,
        score: [0, 0],
        rng: Rng::new(seed),
        pending: Vec::new(),
        talk: [Talked::default(); 2],
        passes: [0, 0],
        pass: None,
        passed: vec![None; 2 * N],
        chase: vec![false; 2 * N],
        frames: Vec::new(),
        events: Vec::new(),
        order: None,
    };
    let first = (m.rng.unit() * 2.0) as usize;
    m.reset(first);
    let steps = (opt.seconds / DT).round() as usize;
    for _ in 0..steps {
        m.step();
    }
    let pending = std::mem::take(&mut m.pending);
    for p in pending {
        m.settle(p, false);
    }
    Outcome {
        score: m.score,
        lex: [m.teams[0].lex, m.teams[1].lex],
        talk: m.talk,
        passes: m.passes,
        frames: m.frames,
        events: m.events,
    }
}

#[cfg(test)]
mod mirror {
    use super::*;
    // Swapping the teams and mirroring the pitch in x must mirror every decision.
    #[test]
    fn decisions_mirror() {
        let opt = Options { comms: [false, false], record: false, calls: [true; 3], seconds: 90.0 };
        let mut rng = Rng::new(9);
        let a = Team::random(&mut rng);
        let b = Team::random(&mut rng);
        for trial in 0..2000 {
            let mk = |teams: [&Team; 2]| {
                let mut p = Vec::new();
                for team in 0..2 {
                    for idx in 0..N {
                        p.push(Player { team, idx, x: 0.0, y: 0.0, cooldown: 0.0, gap: 0.0, no_chase_until: 0.0, back_until: 0.0, call: None, kb: (W / 2.0, H / 2.0), k_theirs: false, k_at: 0.0 });
                    }
                }
                Match { teams: [teams[0].clone(), teams[1].clone()], opt: &opt, p, bx: 0.0, by: 0.0, bvx: 0.0, bvy: 0.0, last_touch: None, touch_t: 0.0, t: 0.0, score: [0, 0], rng: Rng::new(5), pending: Vec::new(), talk: [Talked::default(); 2], passes: [0, 0], pass: None, passed: vec![None; 2 * N], chase: vec![false; 2 * N], frames: Vec::new(), events: Vec::new(), order: None }
            };
            let mut m1 = mk([&a, &b]);
            let mut m2 = mk([&b, &a]);
            for i in 0..6 {
                let (x, y) = (rng.unit() * W, rng.unit() * H);
                m1.p[i].x = x;
                m1.p[i].y = y;
                let j = (i + 3) % 6;
                m2.p[j].x = W - x;
                m2.p[j].y = y;
            }
            let (bx, by) = (rng.unit() * W, rng.unit() * H);
            m1.bx = bx; m1.by = by; m2.bx = W - bx; m2.by = by;
            m1.t = 1.0; m2.t = 1.0;
            m1.look(); m2.look();
            m1.order = Some(0); m2.order = Some(3);
            m1.rng = Rng::new(trial); m2.rng = Rng::new(trial);
            for _ in 0..1 { m1.step(); m2.step(); }
            for i in 0..6 {
                let j = (i + 3) % 6;
                assert!((m1.p[i].x - (W - m2.p[j].x)).abs() < 1e-6 && (m1.p[i].y - m2.p[j].y).abs() < 1e-6, "step trial {trial} duck {i}: {:?} vs {:?}", (m1.p[i].x, m1.p[i].y), (W - m2.p[j].x, m2.p[j].y));
            }
            assert!((m1.bx - (W - m2.bx)).abs() < 1e-6 && (m1.by - m2.by).abs() < 1e-6, "ball trial {trial}: {:?} vs {:?}", (m1.bx, m1.by), (W - m2.bx, m2.by));
            for i in 0..6 {
                let j = (i + 3) % 6;
                m1.rng = Rng::new(trial); m2.rng = Rng::new(trial);
                let (t1, k1) = m1.decide(i);
                let (t2, k2) = m2.decide(j);
                let bad = (t1.0 - (W - t2.0)).abs() > 1e-6 || (t1.1 - t2.1).abs() > 1e-6
                    || k1.is_some() != k2.is_some()
                    || k1.zip(k2).is_some_and(|(p, q)| (p.0 + q.0).abs() > 1e-6 || (p.1 - q.1).abs() > 1e-6);
                assert!(!bad, "trial {trial} duck {i}: {t1:?} {k1:?} vs mirrored {t2:?} {k2:?}");
            }
        }
    }
}
