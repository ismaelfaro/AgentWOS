//! Duck pond, live: the stage a browser kernel drives tick by tick.
//!
//! The bodies are the same as in the turn-based scenario (`body.rs` is shared).
//! What changes is the brain: here each duck is an `actor` with a small
//! tool-calling model (Needle), which turns one plain sentence into one tool
//! call. The sentence comes from a person ("Pip walk to the cushion") or from
//! the duck itself: when a duck is idle, its body puts its strongest urge into
//! words ("Nilo take a nap") and the same brain acts on it.
//!
//! The model does not reason about what to want. The urges are rules, in the
//! spirit of autod's energy model; the model's job is language to action.

wit_bindgen::generate!({
    world: "stage-module",
    path: "../../../wit/world.wit",
});

// The scenario uses parts of the body that the live stage does not.
#[allow(dead_code)]
#[path = "../../scenario/src/body.rs"]
mod body;

use std::cell::RefCell;

use body::{Ball, Decision, Duck, Policy, Room, Rng, DT};
use exports::agentwos::core::stage::Guest;
use serde_json::{json, Value};

struct Component;

/// Where a duck can be told to go: a spot next to each thing, not inside it.
const PLACES: [(&str, f64, f64); 6] = [
    ("cushion", 3.4, 2.6),
    ("sofa", 0.8, 2.0),
    ("table", 2.6, 1.5),
    ("plant", 1.0, 0.85),
    ("shelf", 3.25, 0.6),
    ("center", 1.9, 1.3),
];

/// Urge weights: walk, zoomies, look around, quack, nap, ball.
struct Profile {
    name: &'static str,
    colour: &'static str,
    trait_: &'static str,
    weights: [f64; 6],
    start: (f64, f64, f64),
}

const DUCKS: [Profile; 4] = [
    Profile { name: "Pip", colour: "blue", trait_: "Curious: explores places it knows least", weights: [5.0, 1.0, 2.0, 1.0, 0.5, 1.0], start: (1.0, 1.4, 0.0) },
    Profile { name: "Momo", colour: "grey", trait_: "Playful: loves the ball and the zoomies", weights: [1.5, 3.0, 1.0, 1.0, 0.5, 4.0], start: (2.7, 0.6, 1.57) },
    Profile { name: "Nilo", colour: "white", trait_: "Sleepy: naps often", weights: [1.5, 0.3, 2.0, 0.7, 4.0, 0.5], start: (3.2, 2.2, 3.14) },
    Profile { name: "Lila", colour: "purple", trait_: "Chatty: quacks, and quacks back", weights: [2.0, 1.0, 1.5, 4.0, 0.5, 1.0], start: (1.8, 2.1, -1.57) },
];

const HEAR_M: f64 = 3.0;
const TIRED: f64 = 0.3;
const RESTED: f64 = 0.95;

/// The ducks' words. They start with none: each duck learns which word means which place.
const SYMBOLS: [&str; 4] = ["chirp", "trill", "honk", "purr"];
const N_SYM: usize = SYMBOLS.len();
const N_MEAN: usize = PLACES.len();
/// Chance that a speaker tries a word at random instead of its best guess: exploration.
const EXPLORE: f64 = 0.2;

struct Mind {
    /// The tool the duck is running, as the UI names it.
    action: &'static str,
    place: Option<&'static str>,
    action_s: f64,
    /// Seconds idle; negative while waiting for the brain to answer an urge.
    idle_s: f64,
    next_urge_s: f64,
    heard: Option<(&'static str, f64)>,
    visits: [u32; PLACES.len()],
    last_event: (String, f64),
    /// The place this duck is walking to, until it says so once.
    target_k: Option<usize>,
    said: bool,
    /// The last word heard: (word, from duck, time).
    heard_sym: Option<(usize, usize, f64)>,
    /// A walk this duck started because it heard a word: (word, place), reinforced on arrival.
    follow: Option<(usize, usize)>,
    /// Co-occurrence counts: how often a word was heard or said with a place.
    lex: [[f64; N_MEAN]; N_SYM],
}

struct Pond {
    room: Room,
    ducks: Vec<Duck>,
    minds: Vec<Mind>,
    ball: Ball,
    t: f64,
}

thread_local! {
    static POND: RefCell<Option<Pond>> = const { RefCell::new(None) };
}

fn with_pond<R>(f: impl FnOnce(&mut Pond) -> R) -> R {
    POND.with(|p| f(p.borrow_mut().as_mut().expect("setup must run first")))
}

fn function(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "type": "function", "function": {
        "name": name, "description": description,
        "parameters": { "type": "object", "properties": properties, "required": required },
    }})
}

fn tools() -> Value {
    let places: Vec<&str> = PLACES.iter().map(|p| p.0).collect();
    json!([
        function("walk_to", "Walk or go to a place in the room.",
            json!({ "place": { "type": "string", "enum": places } }), &["place"]),
        function("kick_ball", "Go to the ball and kick it. Play with the ball.", json!({}), &[]),
        function("nap", "Take a nap: sit down and sleep or rest.", json!({}), &[]),
        function("zoomies", "Do the zoomies: run around fast.", json!({}), &[]),
        function("look_around", "Look around.", json!({}), &[]),
        function("stop", "Stop and stand still.", json!({}), &[]),
    ])
}

fn duck_state(p: &Pond, i: usize) -> Value {
    let (d, m) = (&p.ducks[i], &p.minds[i]);
    json!({
        "agent": d.name,
        "x": d.x, "y": d.y, "heading": d.heading,
        "energy": d.energy,
        "policy": d.policy.name(),
        "action": m.action,
        "place": m.place,
        "goal": d.decision.goal.map(|(x, y)| json!([x, y])),
        "walking": d.policy == Policy::Walk,
    })
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

/// The duck's strongest urge, in words its brain can act on, and why. An empty
/// text is a quack: the body does it itself, since small models cannot pick it reliably.
fn urge(p: &mut Pond, i: usize) -> (String, String) {
    let profile = &DUCKS[i];
    let name = profile.name;
    let pos = (p.ducks[i].x, p.ducks[i].y);
    let energy = p.ducks[i].energy;

    if p.ducks[i].policy == Policy::Sit {
        return (format!("{name} look around"), "rested after the nap".into());
    }
    if energy < TIRED {
        return (format!("{name} take a nap"), format!("energy is at {:.0}%", energy * 100.0));
    }
    // A word from another duck: walk to the place it means, if this duck has learned that word.
    if let Some((s, from, at)) = p.minds[i].heard_sym.take() {
        if p.t - at <= 1.5 {
            let (k, confidence) = interpret(&p.minds[i].lex, s);
            if confidence > 0.4 && p.ducks[i].rng.unit() < 0.6 {
                p.minds[i].follow = Some((s, k));
                return (
                    format!("{name} walk to the {}", PLACES[k].0),
                    format!("heard {} from {}, which means {}", SYMBOLS[s], p.ducks[from].name, PLACES[k].0),
                );
            }
        }
    }
    if let Some((from, _)) = p.minds[i].heard.take() {
        // Every duck answers sometimes; the chatty one almost always.
        let chance = if profile.weights[3] >= 3.0 { 0.9 } else { 0.4 };
        if p.ducks[i].rng.unit() < chance {
            return (String::new(), format!("heard {from} quack"));
        }
    }
    let ball_near = dist(pos, (p.ball.x, p.ball.y)) < 1.0;

    let mut w = profile.weights;
    if energy < 0.6 {
        w[1] = 0.0; // zoomies are for a duck with energy to burn
    } else {
        w[4] = 0.0; // and naps for one without
    }
    if ball_near {
        w[5] *= 3.0;
    }
    let total: f64 = w.iter().sum();
    let mut roll = p.ducks[i].rng.unit() * total;
    let mut choice = 0;
    for (k, weight) in w.iter().enumerate() {
        if roll < *weight {
            choice = k;
            break;
        }
        roll -= weight;
    }
    match choice {
        0 => {
            // The place visited least, and not the one it is standing at.
            let visits = p.minds[i].visits;
            let noise: Vec<f64> = (0..PLACES.len()).map(|_| p.ducks[i].rng.unit()).collect();
            let (k, _) = PLACES
                .iter()
                .enumerate()
                .filter(|(_, pl)| dist(pos, (pl.1, pl.2)) > 0.5)
                .map(|(k, _)| (k, visits[k] as f64 + noise[k]))
                .fold((0, f64::MAX), |best, c| if c.1 < best.1 { c } else { best });
            (format!("{name} walk to the {}", PLACES[k].0), format!("has been to the {} least", PLACES[k].0))
        }
        1 => (format!("{name} do the zoomies"), "full of energy".into()),
        2 => (format!("{name} look around"), "curious about the room".into()),
        3 => (String::new(), "wants to be heard".into()),
        4 => (format!("{name} take a nap"), "feels like resting".into()),
        _ => (format!("{name} kick the ball"), if ball_near { "the ball is close".into() } else { "wants to play".into() }),
    }
}

fn argmax(row: &[f64]) -> usize {
    let mut best = 0;
    for (k, v) in row.iter().enumerate() {
        if *v > row[best] {
            best = k;
        }
    }
    best
}

/// The word a duck says for a place: its best guess, sometimes a random one.
fn speak(lex: &[[f64; N_MEAN]; N_SYM], k: usize, rng: &mut Rng) -> usize {
    if rng.unit() < EXPLORE {
        return ((rng.unit() * N_SYM as f64) as usize).min(N_SYM - 1);
    }
    let mut best = 0;
    let mut best_score = f64::MIN;
    for s in 0..N_SYM {
        // A little noise breaks ties between words the duck has never tried.
        let score = lex[s][k] + rng.unit() * 0.01;
        if score > best_score {
            best_score = score;
            best = s;
        }
    }
    best
}

/// What a duck takes a word to mean, and how sure it is (share of its counts).
fn interpret(lex: &[[f64; N_MEAN]; N_SYM], s: usize) -> (usize, f64) {
    let row = lex[s];
    let total: f64 = row.iter().sum();
    let k = argmax(&row);
    (k, row[k] / total)
}

/// What the ducks have learned: each duck's best meaning per word, and how often
/// pairs of ducks agree on a word they both know.
fn language(p: &Pond) -> Value {
    let tops: Vec<Vec<Option<usize>>> = p
        .minds
        .iter()
        .map(|m| {
            (0..N_SYM)
                .map(|s| {
                    // A word means a place only when the duck clearly prefers that place
                    // over every other: a word heard only with the cushion means nothing yet.
                    let row = m.lex[s];
                    let top = argmax(&row);
                    let second = row.iter().enumerate().filter(|(k, _)| *k != top).map(|(_, v)| *v).fold(f64::MIN, f64::max);
                    if row[top] - second >= 2.0 { Some(top) } else { None }
                })
                .collect()
        })
        .collect();
    let (mut same, mut pairs) = (0usize, 0usize);
    let n = p.ducks.len();
    for s in 0..N_SYM {
        for a in 0..n {
            for b in a + 1..n {
                if let (Some(x), Some(y)) = (tops[a][s], tops[b][s]) {
                    pairs += 1;
                    if x == y {
                        same += 1;
                    }
                }
            }
        }
    }
    json!({
        "words": SYMBOLS,
        "places": PLACES.iter().map(|pl| pl.0).collect::<Vec<_>>(),
        "ducks": (0..n).map(|i| json!({
            "agent": p.ducks[i].name,
            "means": (0..N_SYM).map(|s| tops[i][s].map(|k| PLACES[k].0)).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "agreement": if pairs > 0 { json!(same as f64 / pairs as f64) } else { Value::Null },
    })
}

impl Guest for Component {
    fn setup(_config: String) -> String {
        let room = Room::apartment();
        let ducks: Vec<Duck> = DUCKS
            .iter()
            .enumerate()
            .map(|(i, d)| Duck::new(d.name, d.start.0, d.start.1, d.start.2, 11 + i as u64, &room))
            .collect();
        let minds = (0..ducks.len())
            .map(|i| Mind {
                action: "stop",
                place: None,
                action_s: 0.0,
                idle_s: 0.0,
                // Staggered, so the four brains do not all speak in the same tick.
                next_urge_s: 1.5 + i as f64 * 1.2,
                heard: None,
                visits: [0; PLACES.len()],
                last_event: (String::new(), -10.0),
                target_k: None,
                said: false,
                heard_sym: None,
                follow: None,
                lex: [[0.5; N_MEAN]; N_SYM],
            })
            .collect();
        let pond = Pond { room, ducks, minds, ball: Ball::new(1.5, 1.4), t: 0.0 };
        let out = json!({
            "agents": DUCKS.iter().map(|d| json!({ "agent": d.name, "role": d.trait_, "colour": d.colour })).collect::<Vec<_>>(),
            "room": pond.room.json(),
            "places": PLACES.iter().map(|p| json!({ "name": p.0, "at": [p.1, p.2] })).collect::<Vec<_>>(),
            "tick_s": DT,
            "suggestions": [
                "Pip walk to the cushion",
                "Momo kick the ball",
                "Nilo take a nap",
                "Momo do the zoomies",
                "Pip look around and Lila walk to the plant",
                "Everyone walk to the table",
                "Everyone stop",
            ],
        });
        POND.with(|p| *p.borrow_mut() = Some(pond));
        out.to_string()
    }

    fn brief(agent: u32) -> String {
        let d = &DUCKS[agent as usize];
        json!({
            // The same words for every duck: the engine keeps the prompt it prepared and
            // reuses it while system and tools do not change (680 ms saved per call).
            // The aliases below already keep each duck to its own requests.
            "system": "You are a small robot duck. Act on the request.",
            // A clause reaches this duck when it says its name, its colour, or everyone.
            "aliases": [d.name.to_lowercase(), d.colour, "everyone", "all", "ducks"],
            "tools": tools(),
        })
        .to_string()
    }

    /// Every duck's body state by name: a control finds its author's entry.
    fn context() -> String {
        with_pond(|p| {
            let mut ducks = serde_json::Map::new();
            for i in 0..p.ducks.len() {
                let d = &p.ducks[i];
                ducks.insert(
                    d.name.to_string(),
                    json!({
                        "policy": d.policy.name(),
                        "energy": d.energy,
                        "tof_ahead_m": body::tof_ahead(&p.room, &p.ducks, i, 0.0).map(|(m, _)| m),
                    }),
                );
            }
            json!({ "ducks": ducks, "room": [p.room.w, p.room.h] }).to_string()
        })
    }

    fn apply(agent: u32, call: String) -> Result<String, String> {
        let call: Value = serde_json::from_str(&call).map_err(|e| format!("call is not JSON: {e}"))?;
        let tool = call["name"].as_str().unwrap_or("");
        with_pond(|p| {
            let i = agent as usize;
            let name = p.ducks[i].name;
            let (action, state, goal, place, said): (&'static str, &str, Option<(f64, f64)>, Option<&'static str>, String) = match tool {
                "walk_to" => {
                    let wanted = call["arguments"]["place"].as_str().unwrap_or("");
                    let (k, pl) = PLACES
                        .iter()
                        .enumerate()
                        .find(|(_, pl)| pl.0 == wanted)
                        .ok_or_else(|| format!("`place` must be one of: {}", PLACES.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")))?;
                    p.minds[i].visits[k] += 1;
                    p.minds[i].target_k = Some(k);
                    p.minds[i].said = false;
                    ("walk_to", "wander", Some((pl.1, pl.2)), Some(pl.0), format!("{name} walks to the {}", pl.0))
                }
                "kick_ball" => ("kick_ball", "ball", None, None, format!("{name} goes for the ball")),
                "nap" => ("nap", "nap", None, None, format!("{name} settles down for a nap")),
                "zoomies" => ("zoomies", "zoomies", None, None, format!("{name} gets the zoomies")),
                "look_around" => ("look_around", "look_around", None, None, format!("{name} looks around")),
                "stop" => ("stop", "chill", None, None, format!("{name} stops")),
                other => return Err(format!("unknown tool '{other}'")),
            };
            p.ducks[i].decision = Decision { state: state.into(), goal, thought: String::new() };
            p.ducks[i].start_round();
            let m = &mut p.minds[i];
            m.action = action;
            if action != "walk_to" {
                m.target_k = None;
            }
            m.place = place;
            m.action_s = 0.0;
            m.idle_s = 0.0;
            Ok(said)
        })
    }

    fn spawn(_kind: String) -> Result<String, String> {
        Err("the pond has room for four ducks".into())
    }

    /// Advances the pond one tick and returns the frame: poses, what happened,
    /// and the urges of idle ducks.
    fn render(_tick: u32) -> String {
        with_pond(|p| {
            let n = p.ducks.len();
            let step = (p.t / DT).round() as usize;
            for k in 0..n {
                body::step(&p.room, &mut p.ducks, &mut p.ball, (k + step) % n);
            }
            body::step_ball(&p.room, &mut p.ball);
            p.t += DT;

            let mut events = Vec::new();
            let mut urges = Vec::new();
            for i in 0..n {
                body::after_step(&p.room, &mut p.ducks[i], false);
                let happened = std::mem::take(&mut p.ducks[i].events);
                let mut finished = false;
                for e in happened {
                    finished |= e == "arrived at its goal" || e == "kicked the ball";
                    if e == "quacked" {
                        for j in 0..n {
                            let d = dist((p.ducks[i].x, p.ducks[i].y), (p.ducks[j].x, p.ducks[j].y));
                            if j != i && d <= HEAR_M {
                                p.minds[j].heard = Some((p.ducks[i].name, p.t));
                            }
                        }
                    }
                    // The body repeats some reports every tick while they hold; say each once.
                    let m = &mut p.minds[i];
                    // A walk that a heard word started, and that arrived, teaches the word.
                    if e == "arrived at its goal" {
                        if let (Some((s, _)), Some(k)) = (m.follow, m.target_k) {
                            m.lex[s][k] += 2.0;
                        }
                        m.follow = None;
                    }
                    if m.last_event.0 != e || p.t - m.last_event.1 > 4.0 {
                        m.last_event = (e.clone(), p.t);
                        events.push(json!({ "agent": p.ducks[i].name, "text": e }));
                    }
                }

                let policy = p.ducks[i].policy;
                let energy = p.ducks[i].energy;
                let m = &mut p.minds[i];
                m.action_s += DT;
                if m.heard.is_some_and(|(_, at)| p.t - at > 6.0) {
                    m.heard = None;
                }
                // How each action ends on its own.
                finished |= match m.action {
                    "quack" => m.action_s > 2.5,
                    "look_around" => m.action_s > 4.0,
                    "zoomies" => m.action_s > 6.0,
                    "walk_to" | "kick_ball" => m.action_s > 25.0,
                    _ => false,
                };
                let down = matches!(policy, Policy::Fallen | Policy::Limp);
                if finished && !down && m.action != "nap" {
                    m.action = "stop";
                    m.place = None;
                    m.target_k = None;
                    m.follow = None;
                    m.idle_s = 0.0;
                    p.ducks[i].decision = Decision { state: "chill".into(), goal: None, thought: String::new() };
                    p.ducks[i].start_round();
                }

                let m = &mut p.minds[i];
                let idle = m.action == "stop" || (m.action == "nap" && energy >= RESTED);
                if idle && !down {
                    m.idle_s += DT;
                    if m.idle_s >= m.next_urge_s || m.heard_sym.is_some() {
                        // Wait for the brain; ask again later if nothing came of it.
                        m.idle_s = -6.0;
                        m.next_urge_s = 1.5 + p.ducks[i].rng.unit() * 3.0;
                        let (text, why) = urge(p, i);
                        if text.is_empty() {
                            let m = &mut p.minds[i];
                            m.action = "quack";
                            m.place = None;
                            m.action_s = 0.0;
                            m.idle_s = 0.0;
                            p.ducks[i].decision = Decision { state: "quack".into(), goal: None, thought: String::new() };
                            p.ducks[i].start_round();
                            events.push(json!({ "agent": p.ducks[i].name, "text": format!("quacks: {why}") }));
                        } else {
                            urges.push(json!({ "agent": i, "text": text, "why": why }));
                        }
                    }
                }
            }

            // Speech: a duck walking to a place says a word for it. Ducks within earshot hear it and learn it.
            for i in 0..n {
                let Some(k) = p.minds[i].target_k else { continue };
                if p.minds[i].said {
                    continue;
                }
                p.minds[i].said = true;
                let lex = p.minds[i].lex;
                let s = speak(&lex, k, &mut p.ducks[i].rng);
                p.minds[i].lex[s][k] += 1.0;
                events.push(json!({ "agent": p.ducks[i].name, "text": format!("says {} (it means {})", SYMBOLS[s], PLACES[k].0) }));
                let here = (p.ducks[i].x, p.ducks[i].y);
                for j in 0..n {
                    if j != i && dist(here, (p.ducks[j].x, p.ducks[j].y)) <= HEAR_M {
                        p.minds[j].lex[s][k] += 1.0;
                        p.minds[j].heard_sym = Some((s, i, p.t));
                    }
                }
            }

            json!({
                "t": p.t,
                "ducks": (0..n).map(|i| duck_state(p, i)).collect::<Vec<_>>(),
                "ball": [p.ball.x, p.ball.y],
                "events": events,
                "urges": urges,
                "language": language(p),
            })
            .to_string()
        })
    }

    fn state() -> String {
        with_pond(|p| {
            json!({
                "t": p.t,
                "ducks": (0..p.ducks.len()).map(|i| duck_state(p, i)).collect::<Vec<_>>(),
                "ball": [p.ball.x, p.ball.y],
                "language": language(p),
            })
            .to_string()
        })
    }
}

export!(Component);
