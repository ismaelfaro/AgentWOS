//! Duck soccer: teams of MicroDucks that learn to play and to talk.
//!
//! A generation is a population of teams. Every team plays a few matches, all
//! teams at once on separate threads, and keeps the words it learned on the way.
//! The best teams survive; the next generation is made of them (elites), of
//! changed copies of them (mutants) and of one team that starts from nothing
//! (a newcomer, as a control).
//!
//! Every generation, the best team also plays the same fixed opponent twice over:
//! once talking and once silent. The difference is what talking is worth.

mod sim;

use std::path::PathBuf;
use std::thread;

use serde_json::{json, Value};

use sim::{play, Lexicon, Options, Rng, Team, G, MEANINGS, N, SOUNDS};

#[derive(Clone)]
struct Member {
    id: String,
    kind: &'static str,
    parent: Option<String>,
    team: Team,
}

#[derive(Default, Clone, Copy)]
struct Tally {
    w: u32,
    d: u32,
    l: u32,
    gf: u32,
    ga: u32,
}

impl Tally {
    fn add(&mut self, mine: u32, theirs: u32) {
        self.gf += mine;
        self.ga += theirs;
        match mine.cmp(&theirs) {
            std::cmp::Ordering::Greater => self.w += 1,
            std::cmp::Ordering::Equal => self.d += 1,
            std::cmp::Ordering::Less => self.l += 1,
        }
    }
    fn games(&self) -> u32 {
        self.w + self.d + self.l
    }
    fn json(&self) -> Value {
        let n = self.games().max(1) as f64;
        json!({ "w": self.w, "d": self.d, "l": self.l, "gf": self.gf, "ga": self.ga,
                "win_rate": self.w as f64 / n, "goal_diff": (self.gf as f64 - self.ga as f64) / n })
    }
}

fn arg<T: std::str::FromStr>(name: &str, default: T) -> T {
    let args: Vec<String> = std::env::args().collect();
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// Plays `me` against a list of opponents, alternating sides, and keeps the words
/// `me` learned from one match to the next.
fn season(me: &Team, talks: bool, opponents: &[(Team, bool)], seed: u64, seconds: f64) -> (Team, Tally, sim::Talked, u32) {
    let mut team = me.clone();
    let mut tally = Tally::default();
    let mut talk = sim::Talked::default();
    let mut passes = 0;
    for (k, (opp, opp_talks)) in opponents.iter().enumerate() {
        let side = k % 2;
        // The chasers never talk, here or in the evaluation: the same opponent both times.
        let mut comms = [*opp_talks; 2];
        comms[side] = talks;
        let opt = Options { comms, record: false, seconds, calls: [true; 3] };
        let teams = if side == 0 { [&team, opp] } else { [opp, &team] };
        let out = play(teams, seed.wrapping_add(k as u64 * 7919), &opt);
        tally.add(out.score[side], out.score[1 - side]);
        team.lex = out.lex[side];
        let t = out.talk[side];
        talk.said += t.said;
        talk.ok += t.ok;
        talk.missed += t.missed;
        talk.correct += t.correct;
        passes += out.passes[side];
    }
    (team, tally, talk, passes)
}

/// `games` matches against the chasers, half on each side, talking or silent.
fn versus_chasers(team: &Team, talking: bool, calls: [bool; 3], games: usize, seed: u64, seconds: f64) -> Tally {
    let chasers = Team::chasers();
    let results: Vec<(u32, u32)> = thread::scope(|s| {
        let handles: Vec<_> = (0..games)
            .map(|k| {
                let chasers = &chasers;
                s.spawn(move || {
                    let side = k % 2;
                    let mut comms = [false, false];
                    comms[side] = talking;
                    let opt = Options { comms, record: false, seconds, calls };
                    let teams = if side == 0 { [team, chasers] } else { [chasers, team] };
                    let out = play(teams, seed.wrapping_add(k as u64 * 104_729), &opt);
                    (out.score[side], out.score[1 - side])
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("a match panicked")).collect()
    });
    let mut t = Tally::default();
    for (a, b) in results {
        t.add(a, b);
    }
    t
}

fn mutate(team: &Team, sigma: f64, rng: &mut Rng) -> Team {
    let mut t = team.clone();
    for g in t.genome.iter_mut() {
        for v in g.iter_mut() {
            *v = (*v + sigma * rng.gauss()).clamp(0.0, 1.0);
        }
    }
    // Words are handed down softened, so the child can still change them.
    for l in t.lex.iter_mut() {
        for m in l.iter_mut() {
            for w in m.iter_mut() {
                *w = (w.powf(0.8) * (sigma * 1.5 * rng.gauss()).exp()).clamp(0.05, 50.0);
            }
        }
    }
    for u in t.use_call.iter_mut() {
        *u = (*u + sigma * 1.5 * rng.gauss()).clamp(0.0, 1.0);
    }
    t.reward = (t.reward * (sigma * rng.gauss()).exp()).clamp(0.3, 4.0);
    t.penalty = (t.penalty * (sigma * rng.gauss()).exp()).clamp(0.05, 2.0);
    t
}

fn vocabulary(lex: &[Lexicon; N]) -> Value {
    Value::Array(
        lex.iter()
            .map(|l| {
                Value::Array(
                    l.iter()
                        .enumerate()
                        .map(|(m, row)| {
                            let total: f64 = row.iter().sum();
                            let best = (0..row.len()).max_by(|a, b| row[*a].partial_cmp(&row[*b]).unwrap()).unwrap();
                            json!({ "meaning": MEANINGS[m], "sound": SOUNDS[best], "share": (row[best] / total * 100.0).round() })
                        })
                        .collect(),
                )
            })
            .collect(),
    )
}

/// The share of meanings for which all three ducks use the same sound.
fn shared_words(lex: &[Lexicon; N]) -> f64 {
    let mut agree = 0;
    for m in 0..MEANINGS.len() {
        let best: Vec<usize> = lex
            .iter()
            .map(|l| (0..SOUNDS.len()).max_by(|a, b| l[m][*a].partial_cmp(&l[m][*b]).unwrap()).unwrap())
            .collect();
        if best.iter().all(|b| *b == best[0]) {
            agree += 1;
        }
    }
    agree as f64 / MEANINGS.len() as f64
}

fn main() {
    let pop: usize = arg("--pop", 8).max(4);
    let gens: usize = arg("--gens", 10);
    let seconds: f64 = arg("--seconds", 90.0);
    let peers: usize = arg("--peers", 2);
    let eval_games: usize = arg("--eval", 20);
    let seed: u64 = arg("--seed", 1);
    // A lineage that never talks, to compare with one that does: --talk 0.
    let talk = arg::<u8>("--talk", 1) != 0;
    let out: PathBuf = arg("--out", PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/runs/latest.js")));

    let mut rng = Rng::new(seed);
    let chasers = Team::chasers();
    let mut members: Vec<Member> = (0..pop)
        .map(|k| Member { id: format!("g0-{k}"), kind: "random", parent: None, team: Team::random(&mut rng) })
        .collect();
    let mut generations = Vec::new();
    println!("duck soccer: {gens} generations x {pop} teams, {seconds} s matches, seed {seed}");

    for g in 0..gens {
        // Every team plays the chasers twice (one per side) and `peers` other teams, all at once.
        let snapshot: Vec<Team> = members.iter().map(|m| m.team.clone()).collect();
        let results: Vec<(Team, Tally, sim::Talked, u32)> = thread::scope(|s| {
            let handles: Vec<_> = (0..pop)
                .map(|i| {
                    let mut opponents = vec![(chasers.clone(), false), (chasers.clone(), false)];
                    for k in 1..=peers {
                        opponents.push((snapshot[(i + k * 3) % pop].clone(), talk));
                    }
                    let me = snapshot[i].clone();
                    let match_seed = seed.wrapping_mul(1_000_003).wrapping_add((g * 1000 + i) as u64);
                    s.spawn(move || season(&me, talk, &opponents, match_seed, seconds))
                })
                .collect();
            handles.into_iter().map(|h| h.join().expect("a season panicked")).collect()
        });

        let fitness: Vec<f64> = results
            .iter()
            .map(|(_, t, _, _)| {
                let n = t.games().max(1) as f64;
                (3.0 * t.w as f64 + t.d as f64) / n + 0.25 * (t.gf as f64 - t.ga as f64) / n
            })
            .collect();
        // Teams keep what they learned this generation.
        for (m, r) in members.iter_mut().zip(&results) {
            m.team = r.0.clone();
        }
        let mut order: Vec<usize> = (0..pop).collect();
        order.sort_by(|a, b| fitness[*b].partial_cmp(&fitness[*a]).unwrap());
        let best = order[0];
        let champ = members[best].team.clone();

        // What talking is worth: the same team, the same opponent, talking or silent.
        let eval_seed = seed.wrapping_mul(7).wrapping_add(g as u64 * 31);
        let on = versus_chasers(&champ, true, [true; 3], eval_games, eval_seed, seconds);
        let off = versus_chasers(&champ, false, [true; 3], eval_games, eval_seed, seconds);
        // What each word is worth on its own: talking with only that call.
        let each: Vec<Tally> = (0..3)
            .map(|m| {
                let mut calls = [false; 3];
                calls[m] = true;
                versus_chasers(&champ, true, calls, eval_games, eval_seed, seconds)
            })
            .collect();

        // A match to watch: the best team, talking, against the chasers.
        let replay = play([&champ, &chasers], eval_seed ^ 0xBEEF, &Options { comms: [true, false], record: true, seconds, calls: [true; 3] });

        let talk = results[best].2;
        println!(
            "gen {g:>2}: best {} ({}) fitness {:.2} | vs chasers talking {:.0}% wins, gd {:+.2} | silent {:.0}% wins, gd {:+.2} | calls understood {:.0}%, shared words {:.0}%",
            members[best].id,
            members[best].kind,
            fitness[best],
            on.w as f64 / eval_games as f64 * 100.0,
            (on.gf as f64 - on.ga as f64) / eval_games as f64,
            off.w as f64 / eval_games as f64 * 100.0,
            (off.gf as f64 - off.ga as f64) / eval_games as f64,
            if talk.ok + talk.missed > 0 { talk.correct as f64 / (talk.ok + talk.missed) as f64 * 100.0 } else { 0.0 },
            shared_words(&champ.lex) * 100.0,
        );
        println!(
            "         calls used: {} | only one call: {}",
            MEANINGS.iter().zip(champ.use_call).map(|(m, u)| format!("{m} {:.0}%", u * 100.0)).collect::<Vec<_>>().join(" "),
            MEANINGS.iter().zip(&each).map(|(m, t)| format!("{m} {:.0}% gd {:+.2}", t.w as f64 / eval_games as f64 * 100.0, (t.gf as f64 - t.ga as f64) / eval_games as f64)).collect::<Vec<_>>().join(" | ")
        );

        generations.push(json!({
            "gen": g,
            "best": members[best].id,
            "teams": members.iter().zip(&results).zip(&fitness).map(|((m, r), f)| {
                let t = r.2;
                json!({
                    "id": m.id, "kind": m.kind, "parent": m.parent, "fitness": f,
                    "record": r.1.json(), "passes": r.3,
                    "talk": { "said": t.said, "ok": t.ok, "missed": t.missed, "correct": t.correct },
                    "shared_words": shared_words(&m.team.lex),
                })
            }).collect::<Vec<_>>(),
            "versus_chasers": { "talking": on.json(), "silent": off.json(), "games": eval_games,
                "only": MEANINGS.iter().zip(&each).map(|(m, t)| json!({ "meaning": m, "record": t.json() })).collect::<Vec<_>>() },
            "champion": {
                "genome": champ.genome.iter().map(|g| g.to_vec()).collect::<Vec<_>>(),
                "vocabulary": vocabulary(&champ.lex),
                "shared_words": shared_words(&champ.lex),
                "reward": champ.reward, "penalty": champ.penalty,
                "use_call": MEANINGS.iter().zip(champ.use_call).map(|(m, u)| json!({ "meaning": m, "use": u })).collect::<Vec<_>>(),
            },
            "replay": { "score": replay.score, "frames": replay.frames, "events": replay.events, "passes": replay.passes },
        }));

        // The next generation: two elites, mutants of the top three, and one newcomer.
        let mut next = Vec::with_capacity(pop);
        for (k, &i) in order.iter().take(2).enumerate() {
            next.push(Member { id: format!("g{}-{k}", g + 1), kind: "elite", parent: Some(members[i].id.clone()), team: members[i].team.clone() });
        }
        let mut k = 2;
        while next.len() < pop - 1 {
            let parent = &members[order[(k - 2) % 3]];
            let sigma = 0.05 + 0.1 * ((k - 2) % 3) as f64;
            next.push(Member { id: format!("g{}-{k}", g + 1), kind: "mutant", parent: Some(parent.id.clone()), team: mutate(&parent.team, sigma, &mut rng) });
            k += 1;
        }
        next.push(Member { id: format!("g{}-{k}", g + 1), kind: "newcomer", parent: None, team: Team::random(&mut rng) });
        members = next;
    }

    let data = json!({
        "meta": {
            "pop": pop, "gens": gens, "seconds": seconds, "seed": seed, "talk": talk, "peers": peers, "eval_games": eval_games,
            "meanings": MEANINGS, "sounds": SOUNDS, "genes": G,
            "pitch": { "w": sim::W, "h": sim::H, "goal": sim::GOAL_W }, "frame_s": 0.2,
        },
        "generations": generations,
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("create runs folder");
    }
    std::fs::write(&out, format!("window.SOCCER = {data};\n")).expect("write log");
    println!("log: {}", out.display());
}
