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

use body::{Ball, Decision, Duck, Policy, Room, DT};
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
            "system": format!("You are {}, a small robot duck. Only act on requests for {}.", d.name, d.name),
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
                    m.idle_s = 0.0;
                    p.ducks[i].decision = Decision { state: "chill".into(), goal: None, thought: String::new() };
                    p.ducks[i].start_round();
                }

                let m = &mut p.minds[i];
                let idle = m.action == "stop" || (m.action == "nap" && energy >= RESTED);
                if idle && !down {
                    m.idle_s += DT;
                    if m.idle_s >= m.next_urge_s {
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

            json!({
                "t": p.t,
                "ducks": (0..n).map(|i| duck_state(p, i)).collect::<Vec<_>>(),
                "ball": [p.ball.x, p.ball.y],
                "events": events,
                "urges": urges,
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
            })
            .to_string()
        })
    }
}

export!(Component);
