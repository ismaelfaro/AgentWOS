//! Talk: ducks make sounds, hear each other and learn what the sounds mean.
//!
//! Nobody is given a shared language. Each duck starts with its own accent: a
//! weight for every (meaning, sound) pair. When a duck has something to say it
//! picks a sound by those weights. A listener reads the sound as the meaning it
//! weights highest and acts on it. Whether the listener did what the speaker
//! meant decides the learning: a success strengthens the pair on both sides, a
//! miss weakens the pair the listener used. Over rounds the ducks converge on
//! shared words, and a sound that used to bring a duck over starts to mean
//! "come" to everyone who heard it.

use serde_json::{json, Value};

use crate::body::{Ball, Decision, Duck, Policy, Rng, Room};

pub const MEANINGS: [&str; 5] = ["come", "play", "rest", "alarm", "greet"];
pub const SOUNDS: [&str; 5] = ["quack", "chirp", "honk", "purr", "hiss"];

const HEAR_M: f64 = 3.0;
const SPEAK_GAP_S: f64 = 4.0;
const WINDOW_S: f64 = 6.0;
const START_WEIGHT: f64 = 1.0;
const ACCENT: f64 = 0.6;
const REWARD: f64 = 1.0;
const PENALTY: f64 = 0.5;
const MIN_WEIGHT: f64 = 0.05;
const MAX_WEIGHT: f64 = 50.0;

pub type Lexicon = [[f64; SOUNDS.len()]; MEANINGS.len()];

/// What a population can evolve: how fast ducks learn and how often they speak.
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub reward: f64,
    pub penalty: f64,
    pub gap: f64,
}

impl Default for Params {
    fn default() -> Self {
        Params { reward: REWARD, penalty: PENALTY, gap: SPEAK_GAP_S }
    }
}

/// A listener waiting to see whether it did what the sound asked for.
struct Pending {
    listener: usize,
    speaker: usize,
    meant: usize,
    believed: usize,
    sound: usize,
    start: (f64, f64),
    speaker_at: (f64, f64),
    since: f64,
    seen_at: usize,
}

pub struct Talk {
    params: Params,
    lex: Vec<Lexicon>,
    rng: Vec<Rng>,
    pending: Vec<Option<Pending>>,
    gap: Vec<f64>,
    /// Clock in seconds since the first tick; never reset, so windows span rounds.
    pub clock: f64,
    /// Events of the round in progress, in the order they happened.
    pub log: Vec<Value>,
    /// (listener, speaker, sound) for every sound heard this round.
    pub heard: Vec<(usize, usize, &'static str)>,
    /// Successes and misses so far, for the summary.
    pub learned: usize,
    pub missed: usize,
}

impl Talk {
    pub fn new(n: usize, seed: u64) -> Self {
        Self::with(n, seed, Params::default(), None)
    }

    /// A talk that starts from inherited vocabularies and its own parameters.
    pub fn with(n: usize, seed: u64, params: Params, base: Option<Vec<Lexicon>>) -> Self {
        // Each duck gets its own accent: a small random spread on every pair.
        let lex = base.unwrap_or_else(|| (0..n)
            .map(|i| {
                let mut rng = Rng::new(seed.wrapping_add(1000 + i as u64));
                let mut l: Lexicon = [[0.0; SOUNDS.len()]; MEANINGS.len()];
                for m in l.iter_mut() {
                    for s in m.iter_mut() {
                        *s = START_WEIGHT + rng.unit() * ACCENT;
                    }
                }
                l
            })
            .collect());
        Talk {
            params,
            lex,
            rng: (0..n).map(|i| Rng::new(seed.wrapping_add(2000 + i as u64))).collect(),
            pending: (0..n).map(|_| None).collect(),
            gap: vec![0.0; n],
            clock: 0.0,
            log: Vec::new(),
            heard: Vec::new(),
            learned: 0,
            missed: 0,
        }
    }

    /// Called once per body step. Speaks, hears and settles what is waiting.
    pub fn tick(&mut self, ducks: &mut [Duck], ball: &Ball, room: &Room, dt: f64) {
        self.clock += dt;
        for g in self.gap.iter_mut() {
            *g -= dt;
        }
        for j in 0..ducks.len() {
            self.resolve(j, ducks, ball);
        }
        for i in 0..ducks.len() {
            if self.gap[i] > 0.0 {
                continue;
            }
            if let Some(meant) = meaning_of(&ducks[i], ducks, ball) {
                self.speak(i, meant, ducks, room);
                self.gap[i] = self.params.gap;
            }
        }
    }

    /// Every duck's weights, to carry knowledge into another run (used by the evolution crate).
    #[allow(dead_code)]
    pub fn lexicons(&self) -> Vec<Lexicon> {
        self.lex.clone()
    }

    /// Clears what belongs to one round. Pending listeners carry over.
    pub fn start_round(&mut self) {
        self.log.clear();
        self.heard.clear();
    }

    /// The sound each meaning mostly uses for one duck, with its share.
    pub fn vocabulary(&self, i: usize) -> Value {
        let l = &self.lex[i];
        let rows: Vec<Value> = MEANINGS
            .iter()
            .enumerate()
            .map(|(m, meaning)| {
                let total: f64 = l[m].iter().sum();
                let best = argmax(&l[m]);
                json!({
                    "meaning": meaning,
                    "sound": SOUNDS[best],
                    "share": (l[m][best] / total * 100.0).round(),
                })
            })
            .collect();
        Value::Array(rows)
    }

    fn pick(&mut self, i: usize, meant: usize) -> usize {
        let weights = self.lex[i][meant];
        let total: f64 = weights.iter().sum();
        let mut roll = self.rng[i].unit() * total;
        for (s, w) in weights.iter().enumerate() {
            if roll < *w {
                return s;
            }
            roll -= w;
        }
        weights.len() - 1
    }

    fn speak(&mut self, i: usize, meant: usize, ducks: &mut [Duck], room: &Room) {
        let sound = self.pick(i, meant);
        let speaker_at = (ducks[i].x, ducks[i].y);
        let mut heard_by = Vec::new();
        for j in 0..ducks.len() {
            if j == i || dist(speaker_at, (ducks[j].x, ducks[j].y)) > HEAR_M {
                continue;
            }
            let believed = interpret(&self.lex[j], sound);
            // A new sound settles whatever the listener was still waiting on.
            if let Some(old) = self.pending[j].take() {
                self.settle(old, false, ducks);
            }
            act(&mut ducks[j], believed, sound, speaker_at, room);
            self.pending[j] = Some(Pending {
                listener: j,
                speaker: i,
                meant,
                believed,
                sound,
                start: (ducks[j].x, ducks[j].y),
                speaker_at,
                since: self.clock,
                seen_at: ducks[j].events.len(),
            });
            self.heard.push((j, i, SOUNDS[sound]));
            heard_by.push(json!({ "listener": ducks[j].name, "believed": MEANINGS[believed] }));
        }
        self.log.push(json!({
            "t": self.clock,
            "event": "said",
            "speaker": ducks[i].name,
            "x": speaker_at.0,
            "y": speaker_at.1,
            "sound": SOUNDS[sound],
            "meant": MEANINGS[meant],
            "heard_by": heard_by,
        }));
    }

    fn resolve(&mut self, j: usize, ducks: &mut [Duck], ball: &Ball) {
        let Some(p) = self.pending[j].as_ref() else { return };
        let d = &ducks[j];
        let since_events = &d.events[p.seen_at.min(d.events.len())..];
        let speaker_now = (ducks[p.speaker].x, ducks[p.speaker].y);
        let ok = match MEANINGS[p.believed] {
            "come" => dist((d.x, d.y), speaker_now) < dist(p.start, p.speaker_at) - 0.5,
            "play" => since_events.iter().any(|e| e == "kicked the ball"),
            "rest" => d.policy == Policy::Sit,
            "alarm" => dist((d.x, d.y), p.start) > 0.8,
            "greet" => d.decision.state == "quack" || since_events.iter().any(|e| e == "quacked"),
            _ => false,
        };
        let timed_out = self.clock - p.since > WINDOW_S;
        let _ = ball;
        if ok || timed_out {
            let p = self.pending[j].take().expect("checked above");
            self.settle(p, ok, ducks);
        }
    }

    fn settle(&mut self, p: Pending, ok: bool, ducks: &[Duck]) {
        let (l, s) = (p.listener, p.sound);
        if ok {
            bump(&mut self.lex[l][p.believed][s], self.params.reward);
            // The speaker learns only when the listener understood it.
            if p.believed == p.meant {
                bump(&mut self.lex[p.speaker][p.meant][s], self.params.reward);
            }
            self.learned += 1;
        } else {
            bump(&mut self.lex[l][p.believed][s], -self.params.penalty);
            self.missed += 1;
        }
        self.log.push(json!({
            "t": self.clock,
            "event": if ok { "understood" } else { "missed" },
            "listener": ducks[l].name,
            "speaker": ducks[p.speaker].name,
            "sound": SOUNDS[s],
            "meant": MEANINGS[p.meant],
            "believed": MEANINGS[p.believed],
        }));
    }
}

fn meaning_of(d: &Duck, ducks: &[Duck], ball: &Ball) -> Option<usize> {
    if d.decision.state == "quack" {
        return Some(4);
    }
    if d.decision.state == "ball" && dist((d.x, d.y), (ball.x, ball.y)) < 1.0 {
        return Some(1);
    }
    if d.policy == Policy::Sit {
        return Some(2);
    }
    if d.events.last().is_some_and(|e| e.starts_with("ToF stop")) {
        return Some(3);
    }
    if d.decision.state == "wander" {
        let goal = d.decision.goal?;
        let near_someone = ducks.iter().any(|o| o.name != d.name && dist(goal, (o.x, o.y)) < 0.5);
        if near_someone {
            return Some(0);
        }
    }
    None
}

/// What a listener takes a sound to mean: its highest-weighted meaning.
fn interpret(l: &Lexicon, sound: usize) -> usize {
    let column: Vec<f64> = l.iter().map(|m| m[sound]).collect();
    argmax(&column)
}

fn act(d: &mut Duck, believed: usize, sound: usize, toward: (f64, f64), room: &Room) {
    let heard = SOUNDS[sound];
    let (state, goal) = match MEANINGS[believed] {
        "come" => ("wander", Some(toward)),
        "play" => ("ball", None),
        "rest" => ("nap", None),
        "alarm" => {
            // Away from the sound, clamped to the room.
            let (gx, gy) = (d.x + (d.x - toward.0) * 1.5, d.y + (d.y - toward.1) * 1.5);
            ("wander", Some((gx.clamp(0.2, room.w - 0.2), gy.clamp(0.2, room.h - 0.2))))
        }
        _ => ("quack", None),
    };
    d.decision = Decision {
        state: state.into(),
        goal,
        thought: format!("heard {heard}, takes it as {}", MEANINGS[believed]),
    };
}

fn bump(w: &mut f64, by: f64) {
    *w = (*w + by).clamp(MIN_WEIGHT, MAX_WEIGHT);
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

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}
