//! Evolution of the ducks' language.
//!
//! A generation is a population of independent runs, in parallel. Each run is
//! 100 rounds of the four ducks living, talking and learning what sounds mean.
//! The run whose ducks understand each other best survives; its vocabulary (the
//! knowledge it accumulated) and its learning parameters are handed down to the
//! next generation, as an unchanged elite, as mutants, and next to a newcomer
//! that starts from nothing, so the log shows whether inherited knowledge beats
//! starting over.
//!
//! The bodies and the talk are the code the turn-based scenario runs
//! (`scenario/src/body.rs` and `talk.rs`); only the brain is a fast rule-based
//! one (`brain.rs`), because an evolution needs millions of decisions.

#[allow(dead_code)]
#[path = "../../scenario/src/body.rs"]
mod body;
mod brain;
#[allow(dead_code)]
#[path = "../../scenario/src/talk.rs"]
mod talk;

use std::path::PathBuf;
use std::thread;

use serde_json::{json, Value};

use body::{Ball, Room, Rng, DT};
use talk::{Lexicon, Params, Talk, MEANINGS, SOUNDS};

struct Spec {
    id: String,
    kind: &'static str,
    parent: Option<String>,
    seed: u64,
    params: Params,
    base: Option<Vec<Lexicon>>,
}

struct Outcome {
    id: String,
    kind: &'static str,
    parent: Option<String>,
    params: Params,
    curve: Vec<Option<f64>>,
    said: usize,
    fitness: f64,
    first_half: f64,
    second_half: f64,
    interactions: usize,
    convention: f64,
    clarity: f64,
    vocab: Value,
    lex: Vec<Lexicon>,
}

fn gauss(rng: &mut Rng) -> f64 {
    let (u1, u2) = (rng.unit().max(1e-12), rng.unit());
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

/// One run: `rounds` rounds of `round_s` seconds. Returns how well the ducks
/// understood each other, round by round and overall.
fn run(spec: &Spec, rounds: usize, round_s: f64) -> Outcome {
    let room = Room::apartment();
    let mut ducks: Vec<body::Duck> = brain::DUCKS
        .iter()
        .enumerate()
        .map(|(i, p)| body::Duck::new(p.name, p.start.0, p.start.1, p.start.2, spec.seed.wrapping_add(i as u64), &room))
        .collect();
    let mut ball = Ball::new(1.5, 1.4);
    let mut talk = Talk::with(ducks.len(), spec.seed, spec.params, spec.base.clone());
    let mut rng = Rng::new(spec.seed ^ 0xA5A5);
    let steps = (round_s / DT).round() as usize;
    let n = ducks.len();

    let mut curve: Vec<Option<f64>> = Vec::with_capacity(rounds);
    let (mut said, mut half_ok, mut half_all, mut first_ok, mut first_all) = (0usize, 0usize, 0usize, 0usize, 0usize);
    for r in 0..rounds {
        for d in &mut ducks {
            d.start_round();
        }
        talk.start_round();
        brain::decide(&mut ducks, &ball, &room, &mut rng);
        for s in 0..steps {
            for k in 0..n {
                body::step(&room, &mut ducks, &mut ball, (k + s) % n);
            }
            body::step_ball(&room, &mut ball);
            for d in &mut ducks {
                body::after_step(&room, d, false);
            }
            talk.tick(&mut ducks, &ball, &room, DT);
        }
        // Correct = the listener took the sound to mean what the speaker meant.
        let (mut ok, mut all) = (0usize, 0usize);
        for e in &talk.log {
            match e["event"].as_str() {
                Some("said") => said += 1,
                Some("understood") | Some("missed") => {
                    all += 1;
                    if e["believed"] == e["meant"] {
                        ok += 1;
                    }
                }
                _ => {}
            }
        }
        curve.push((all > 0).then(|| ok as f64 / all as f64));
        if r >= rounds / 2 {
            half_ok += ok;
            half_all += all;
        } else {
            first_ok += ok;
            first_all += all;
        }
    }

    let rate = |ok: usize, all: usize| if all == 0 { 0.0 } else { ok as f64 / all as f64 };
    let second_half = rate(half_ok, half_all);
    let lex = talk.lexicons();
    let (convention, clarity) = shared_language(&lex);
    // Success is being understood in a language the whole flock shares. Being
    // understood alone saturates in a generation: ducks can read each other with
    // four different dictionaries. A run where nobody spoke much proves nothing,
    // so its score is scaled down.
    let fitness = second_half * (0.4 + 0.6 * convention) * (half_all as f64 / 30.0).min(1.0);
    Outcome {
        id: spec.id.clone(),
        kind: spec.kind,
        parent: spec.parent.clone(),
        params: spec.params,
        curve,
        said,
        fitness,
        first_half: rate(first_ok, first_all),
        second_half,
        interactions: half_all + first_all,
        convention,
        clarity,
        vocab: Value::Array((0..n).map(|i| talk.vocabulary(i)).collect()),
        lex,
    }
}

/// How alike the ducks' languages are. `convention`: for each meaning, the share
/// of ducks whose favourite sound is the most common one. `clarity`: how much of
/// its weight a duck puts on its favourite sound, on average.
fn shared_language(lex: &[Lexicon]) -> (f64, f64) {
    let (mut conv, mut clar, mut cells) = (0.0, 0.0, 0.0);
    for m in 0..MEANINGS.len() {
        let mut votes = [0usize; SOUNDS.len()];
        for l in lex {
            let best = (0..SOUNDS.len()).max_by(|a, b| l[m][*a].partial_cmp(&l[m][*b]).unwrap()).unwrap();
            votes[best] += 1;
            clar += l[m][best] / l[m].iter().sum::<f64>();
            cells += 1.0;
        }
        conv += *votes.iter().max().unwrap() as f64 / lex.len() as f64;
    }
    (conv / MEANINGS.len() as f64, clar / cells)
}

/// Hands knowledge down with some noise: soften first (a language that learned
/// hard can still change), then perturb every weight.
fn mutate(parent: &[Lexicon], sigma: f64, rng: &mut Rng) -> Vec<Lexicon> {
    parent
        .iter()
        .map(|l| {
            let mut out = *l;
            for m in out.iter_mut() {
                for w in m.iter_mut() {
                    *w = (w.powf(0.8) * (sigma * gauss(rng)).exp()).clamp(0.05, 50.0);
                }
            }
            out
        })
        .collect()
}

fn mutate_params(p: Params, sigma: f64, rng: &mut Rng) -> Params {
    let f = |x: f64, lo: f64, hi: f64, rng: &mut Rng| (x * (sigma * 0.6 * gauss(rng)).exp()).clamp(lo, hi);
    Params { reward: f(p.reward, 0.3, 4.0, rng), penalty: f(p.penalty, 0.05, 2.0, rng), gap: f(p.gap, 1.5, 10.0, rng) }
}

fn arg<T: std::str::FromStr>(name: &str, default: T) -> T {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

fn outcome_json(o: &Outcome) -> Value {
    let curve: Vec<Value> = o.curve.iter().map(|v| v.map_or(Value::Null, |x| json!((x * 1000.0).round() / 1000.0))).collect();
    json!({
        "id": o.id,
        "kind": o.kind,
        "parent": o.parent,
        "params": { "reward": o.params.reward, "penalty": o.params.penalty, "gap": o.params.gap },
        "fitness": o.fitness,
        "understanding": o.second_half,
        "first_half": o.first_half,
        "second_half": o.second_half,
        "said": o.said,
        "interactions": o.interactions,
        "convention": o.convention,
        "clarity": o.clarity,
        "curve": curve,
        "vocab": o.vocab,
    })
}

fn main() {
    let pop: usize = arg("--pop", 8).max(3);
    let gens: usize = arg("--gens", 10);
    let rounds: usize = arg("--rounds", 100);
    let round_s: f64 = arg("--round-s", 20.0);
    let seed: u64 = arg("--seed", 1);
    let out: PathBuf = arg("--out", PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../runs/evolution.js")));

    let mut rng = Rng::new(seed);
    // The survivor so far: its vocabulary, its parameters and its id.
    let mut survivor: Option<(Vec<Lexicon>, Params, String)> = None;
    let mut generations = Vec::new();
    println!("evolution: {gens} generations x {pop} runs x {rounds} rounds of {round_s} s, seed {seed}");

    for g in 0..gens {
        let specs: Vec<Spec> = (0..pop)
            .map(|k| {
                let id = format!("g{g}-{k}");
                let run_seed = seed.wrapping_mul(1_000_003).wrapping_add((g * 1000 + k) as u64 + 1);
                match &survivor {
                    None => Spec { id, kind: "random", parent: None, seed: run_seed, params: Params::default(), base: None },
                    Some((lex, params, pid)) if k == 0 => Spec { id, kind: "elite", parent: Some(pid.clone()), seed: run_seed, params: *params, base: Some(lex.clone()) },
                    Some(_) if k == pop - 1 => Spec { id, kind: "newcomer", parent: None, seed: run_seed, params: Params::default(), base: None },
                    Some((lex, params, pid)) => {
                        let sigma = 0.12 + 0.35 * (k - 1) as f64 / (pop - 3).max(1) as f64;
                        Spec { id, kind: "mutant", parent: Some(pid.clone()), seed: run_seed, params: mutate_params(*params, sigma, &mut rng), base: Some(mutate(lex, sigma, &mut rng)) }
                    }
                }
            })
            .collect();

        // The whole population lives at once.
        let outcomes: Vec<Outcome> = thread::scope(|s| {
            let handles: Vec<_> = specs.iter().map(|spec| s.spawn(move || run(spec, rounds, round_s))).collect();
            handles.into_iter().map(|h| h.join().expect("a run panicked")).collect()
        });

        let best = outcomes.iter().max_by(|a, b| a.fitness.partial_cmp(&b.fitness).unwrap()).unwrap();
        let avg = outcomes.iter().map(|o| o.fitness).sum::<f64>() / outcomes.len() as f64;
        println!(
            "gen {g:>2}: best {} ({}) fitness {:.3}, avg {:.3}, convention {:.2}, clarity {:.2}",
            best.id, best.kind, best.fitness, avg, best.convention, best.clarity
        );
        generations.push(json!({
            "gen": g,
            "best": best.id,
            "runs": outcomes.iter().map(outcome_json).collect::<Vec<_>>(),
        }));
        survivor = Some((best.lex.clone(), best.params, best.id.clone()));
    }

    let data = json!({
        "meta": {
            "pop": pop, "gens": gens, "rounds": rounds, "round_s": round_s, "seed": seed,
            "meanings": MEANINGS, "sounds": SOUNDS,
            "ducks": brain::DUCKS.iter().map(|d| d.name).collect::<Vec<_>>(),
        },
        "generations": generations,
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("create runs folder");
    }
    std::fs::write(&out, format!("window.EVOLUTION = {data};\n")).expect("write log");
    println!("log: {}", out.display());
}
