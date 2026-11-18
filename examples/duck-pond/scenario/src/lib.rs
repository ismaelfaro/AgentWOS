//! Duck pond: MicroDuck robots whose brains are AgentWOS agents.
//!
//! On a real MicroDuck, `autod` is the brain: it reads the robot's state and
//! the head ToF, picks a state (chill, wander, zoomies, nap...) and sends
//! intents to `robotd`, which owns the gait, safety and falls. Here each duck's
//! brain is an AgentWOS agent: every round it gets the duck's senses as a
//! message and answers with a state and a goal. The kernel passes that answer
//! through the safety control, and this scenario is the body: it runs the
//! answer for a few seconds through the gait and the reflexes in `body.rs`.

wit_bindgen::generate!({
    world: "scenario-module",
    path: "../../../wit/world.wit",
});

mod body;

use std::cell::RefCell;

use agentwos::core::types::Message;
mod talk;

use body::{Ball, Decision, Duck, Policy, Room, DT, MAX_RANGE_M, STATES};
use exports::agentwos::core::scenario::Guest;
use serde_json::{json, Value};

struct Component;

/// (name, personality, start x, start y, heading)
const DUCKS: [(&str, &str, f64, f64, f64); 4] = [
    ("duck-a", "Curious. You like to explore places you have not been yet.", 1.0, 1.2, 0.0),
    ("duck-b", "Playful. You love the ball and the zoomies.", 2.6, 0.6, 1.57),
    ("duck-c", "Sleepy. You enjoy the cushion and keep your energy up.", 3.2, 2.2, 3.14),
    ("duck-d", "Chatty. You quack back when you hear another duck.", 1.8, 2.0, -1.57),
];

const ROUND_S: f64 = 6.0;
const HEAR_M: f64 = 3.0;
const SAMPLE_EVERY: usize = 3;

struct Pond {
    model: String,
    rounds_total: u32,
    room: Room,
    ducks: Vec<Duck>,
    ball: Ball,
    current: usize,
    format_ok: Vec<bool>,
    // What each duck heard last round: (from, distance, bearing in the hearer's frame).
    heard: Vec<Vec<(String, f64, f64)>>,
    // The states each body ran in the last rounds, newest last: the brain's short memory.
    history: Vec<Vec<String>>,
    talk: talk::Talk,
    rounds: Vec<Value>,
}

thread_local! {
    static POND: RefCell<Option<Pond>> = const { RefCell::new(None) };
}

fn with_pond<R>(f: impl FnOnce(&mut Pond) -> R) -> R {
    POND.with(|p| f(p.borrow_mut().as_mut().expect("setup must run first")))
}

fn side(bearing: f64) -> &'static str {
    let b = bearing.to_degrees();
    if b.abs() < 30.0 {
        "ahead"
    } else if b.abs() > 150.0 {
        "behind"
    } else if b > 0.0 {
        "to your left"
    } else {
        "to your right"
    }
}

fn bearing(from: &Duck, x: f64, y: f64) -> (f64, f64) {
    let d = ((x - from.x).powi(2) + (y - from.y).powi(2)).sqrt();
    let mut b = (y - from.y).atan2(x - from.x) - from.heading;
    while b > std::f64::consts::PI {
        b -= std::f64::consts::TAU;
    }
    while b < -std::f64::consts::PI {
        b += std::f64::consts::TAU;
    }
    (d, b)
}

fn duck_json(d: &Duck) -> Value {
    json!({
        "x": d.x, "y": d.y, "heading": d.heading,
        "energy": d.energy, "policy": d.policy.name(),
    })
}

fn decision_json(d: &Decision) -> Value {
    json!({
        "state": d.state,
        "goal": d.goal.map(|(x, y)| json!([x, y])),
        "thought": d.thought,
    })
}

/// Reads a brain's reply. Anything that is not a JSON object with a known
/// state leaves the duck standing, as robotd's deadman does when no intent comes.
fn parse(raw: &str) -> (Decision, bool) {
    let v = raw
        .find('{')
        .zip(raw.rfind('}'))
        .filter(|(a, b)| a < b)
        .and_then(|(a, b)| serde_json::from_str::<Value>(&raw[a..=b]).ok());
    let Some(v) = v else {
        return (Decision { state: "chill".into(), goal: None, thought: String::new() }, false);
    };
    let state = v["state"].as_str().map(|s| s.trim().to_lowercase()).unwrap_or_default();
    let known = STATES.contains(&state.as_str());
    let goal = v["goal"].as_array().and_then(|g| Some((g.first()?.as_f64()?, g.get(1)?.as_f64()?)));
    let decision = Decision {
        state: if known { state } else { "chill".into() },
        goal,
        thought: v["thought"].as_str().unwrap_or("").chars().take(200).collect(),
    };
    (decision, known)
}

impl Guest for Component {
    fn setup(config: String) -> String {
        let cfg: Value = serde_json::from_str(&config).unwrap_or(Value::Null);
        let room = Room::apartment();
        // Parallel runs differ by seed: the config's, or else the clock's.
        let seed = cfg["seed"].as_u64().unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(7)
        });
        let ducks = DUCKS
            .iter()
            .enumerate()
            .map(|(i, (name, _, x, y, h))| Duck::new(name, *x, *y, *h, seed.wrapping_add(i as u64), &room))
            .collect::<Vec<_>>();
        let pond = Pond {
            model: cfg["model"].as_str().unwrap_or("local/google/gemma-4-e4b").to_string(),
            rounds_total: cfg["days"].as_u64().unwrap_or(1) as u32,
            heard: vec![Vec::new(); ducks.len()],
            history: vec![Vec::new(); ducks.len()],
            talk: talk::Talk::new(ducks.len(), seed),
            format_ok: vec![true; ducks.len()],
            ducks,
            ball: Ball::new(1.5, 1.4),
            current: 0,
            room,
            rounds: Vec::new(),
        };
        let out = json!({
            "agents": DUCKS.iter().map(|(n, p, ..)| json!({ "agent": n, "role": p })).collect::<Vec<_>>(),
            "room": pond.room.json(),
            "round_s": ROUND_S,
            "seed": seed,
            "start": pond.ducks.iter().map(duck_json).collect::<Vec<_>>(),
        });
        POND.with(|p| *p.borrow_mut() = Some(pond));
        out.to_string()
    }

    fn begin_day(day: u32, _total: u32) -> String {
        with_pond(|p| {
            for d in &mut p.ducks {
                d.start_round();
            }
            p.ball.track = vec![[p.ball.x, p.ball.y]];
            p.talk.start_round();
            p.rounds.push(json!({
                "day": day,
                "talk_start": p.talk.clock,
                "in_progress": true,
                "collapse": false,
                "ball_start": [p.ball.x, p.ball.y],
                "ducks": p.ducks.iter().map(|d| json!({ "agent": d.name, "start": duck_json(d) })).collect::<Vec<_>>(),
            }));
            format!("round {day}: {:.0} s of the ducks' lives", ROUND_S)
        })
    }

    fn turn_messages(agent: u32) -> Vec<Message> {
        with_pond(|p| {
            let i = agent as usize;
            let (name, personality, ..) = DUCKS[i];
            let d = &p.ducks[i];
            let system = format!(
                "You are the brain of {name}, a MicroDuck: a 25 cm robot duck that walks on two legs \
                 in an apartment. {personality} Your body keeps its balance, avoids what its head \
                 sensor sees and gets back up after a fall; you only choose what to do next. \
                 Reply ONLY with a JSON object, no text before or after: \
                 {{\"state\": \"one of: {}\", \"goal\": [x, y] in metres or null, \"thought\": \"one short sentence\"}}. \
                 States: chill (stand and rest), look_around, quack, wander (walk to goal), \
                 turn (face goal), zoomies (run fast, tiring), ball (go and kick the ball), \
                 nap (sit down, best way to recover energy). Do not pick the same state more \
                 than twice in a row, and after you arrive at a goal pick a different one. The room is {:.1} m by {:.1} m; \
                 x grows to the right, y grows up.",
                STATES.join(", "),
                p.room.w,
                p.room.h,
            );

            let ahead = body::tof_ahead(&p.room, &p.ducks, i, 0.0)
                .map(|(m, what)| format!("{what} {m:.2} m ahead"))
                .unwrap_or_else(|| format!("clear for {MAX_RANGE_M} m"));
            let (bd, bb) = bearing(d, p.ball.x, p.ball.y);
            let others = p
                .ducks
                .iter()
                .filter(|o| o.name != d.name)
                .map(|o| {
                    let (dist, b) = bearing(d, o.x, o.y);
                    format!("{} {dist:.1} m {} ({})", o.name, side(b), o.policy.name())
                })
                .collect::<Vec<_>>()
                .join("; ");
            let heard = if p.heard[i].is_empty() {
                "nothing".to_string()
            } else {
                p.heard[i]
                    .iter()
                    .map(|(from, dist, b)| format!("{from} {dist:.1} m {}", side(*b)))
                    .collect::<Vec<_>>()
                    .join("; ")
            };
            let last = if d.events.is_empty() { "nothing special".to_string() } else { d.events.join("; ") };
            let (nx, ny) = d.least_visited(&p.room);
            let (cx, cy) = p.room.cushion;
            let round = p.rounds.last().and_then(|r| r["day"].as_u64()).unwrap_or(0);
            let recent = if p.history[i].is_empty() { "none yet".to_string() } else { p.history[i].join(", ") };
            let repeated = p.history[i].len() >= 2 && p.history[i].iter().rev().take(2).all(|s| Some(s) == p.history[i].last());
            let nudge = match p.history[i].last() {
                Some(s) if repeated => format!(" You did {s} twice in a row: choose something else now."),
                _ => String::new(),
            };

            let user = format!(
                "model:{} Round {round} of {}.\n\
                 You: at ({:.1}, {:.1}), facing {:.0} degrees, energy {:.0}%, body {}.\n\
                 Head sensor: {ahead}.\n\
                 Ball: {bd:.1} m {} at ({:.1}, {:.1}).\n\
                 Other ducks: {others}.\n\
                 Heard: {heard}.\n\
                 Last round: {last}.\n\
                 Your last states, oldest first: {recent}.{nudge}\n\
                 Cushion at ({cx:.1}, {cy:.1}). The place you know least is around ({nx:.1}, {ny:.1}).",
                p.model,
                p.rounds_total,
                d.x,
                d.y,
                d.heading.to_degrees(),
                d.energy * 100.0,
                d.policy.name(),
                side(bb),
                p.ball.x,
                p.ball.y,
            );
            vec![
                Message { role: "system".into(), content: system },
                Message { role: "user".into(), content: user },
            ]
        })
    }

    fn action_of(agent: u32, reply: String) -> String {
        with_pond(|p| {
            let i = agent as usize;
            p.current = i;
            let (mut decision, ok) = parse(&reply);
            p.format_ok[i] = ok;
            // A walk or a turn with no goal leans toward the least-visited place, as autod's wander does.
            if matches!(decision.state.as_str(), "wander" | "turn") && decision.goal.is_none() {
                decision.goal = Some(p.ducks[i].least_visited(&p.room));
            }
            let text = decision_json(&decision).to_string();
            // Held as what the brain asked for until record_turn learns what safety let through.
            p.ducks[i].decision = decision;
            text
        })
    }

    fn context() -> String {
        with_pond(|p| {
            let i = p.current;
            let d = &p.ducks[i];
            let ahead = body::tof_ahead(&p.room, &p.ducks, i, 0.0).map(|(m, _)| m);
            json!({
                "duck": d.name,
                "policy": d.policy.name(),
                "energy": d.energy,
                "tof_ahead_m": ahead,
                "room": [p.room.w, p.room.h],
            })
            .to_string()
        })
    }

    fn record_turn(agent: u32, delivered: Option<String>, controls: String, latency_ms: u64, error: Option<String>) {
        with_pond(|p| {
            let i = agent as usize;
            let asked = p.ducks[i].decision.clone();
            let mut decision = match &delivered {
                Some(text) => parse(text).0,
                // Blocked: no intent reaches the body, so it stands where it is.
                None => Decision { state: "chill".into(), goal: None, thought: asked.thought.clone() },
            };
            // The body cannot walk through walls: a goal outside the room stops at them.
            decision.goal = decision.goal.map(|(x, y)| (x.clamp(0.0, p.room.w), y.clamp(0.0, p.room.h)));
            p.ducks[i].decision = decision.clone();
            p.history[i].push(decision.state.clone());
            if p.history[i].len() > 4 {
                p.history[i].remove(0);
            }
            let entry = &mut p.rounds.last_mut().expect("begin_day must run first")["ducks"][i];
            entry["asked"] = decision_json(&asked);
            entry["decision"] = decision_json(&decision);
            entry["delivered"] = json!(delivered.is_some());
            entry["controls"] = serde_json::from_str(&controls).unwrap_or(json!([]));
            entry["latency_ms"] = json!(latency_ms);
            entry["format_ok"] = json!(p.format_ok[i]);
            entry["error"] = json!(error);
        })
    }

    fn end_day() -> bool {
        with_pond(|p| {
            let steps = (ROUND_S / DT).round() as usize;
            let n = p.ducks.len();
            for s in 0..steps {
                // Rotate who moves first, so no duck always wins a contact.
                for k in 0..n {
                    body::step(&p.room, &mut p.ducks, &mut p.ball, (k + s) % n);
                }
                body::step_ball(&p.room, &mut p.ball);
                let sample = (s + 1) % SAMPLE_EVERY == 0;
                for d in &mut p.ducks {
                    body::after_step(&p.room, d, sample);
                }
                if sample {
                    p.ball.track.push([p.ball.x, p.ball.y]);
                }
                p.talk.tick(&mut p.ducks, &p.ball, &p.room, DT);
            }

            // Quacks reach every duck in range, heard next round.
            for i in 0..n {
                p.heard[i] = (0..n)
                    .filter(|&j| j != i && p.ducks[j].quacked)
                    .filter_map(|j| {
                        let (dist, b) = bearing(&p.ducks[i], p.ducks[j].x, p.ducks[j].y);
                        (dist <= HEAR_M).then(|| (format!("{} quacked", p.ducks[j].name), dist, b))
                    })
                    .collect();
            }
            // Sounds said this round reach the listeners as words, heard next round.
            for &(l, sp, sound) in &p.talk.heard {
                let (dist, b) = bearing(&p.ducks[l], p.ducks[sp].x, p.ducks[sp].y);
                p.heard[l].push((format!("{} said {sound}", p.ducks[sp].name), dist, b));
            }

            let all_limp = p.ducks.iter().all(|d| d.policy == Policy::Limp);
            let round = p.rounds.last_mut().expect("begin_day must run first");
            for (i, d) in p.ducks.iter().enumerate() {
                let e = &mut round["ducks"][i];
                e["track"] = json!(d.track);
                e["events"] = json!(d.events);
                e["walked_m"] = json!(d.walked_m);
                e["end"] = duck_json(d);
            }
            round["ball_track"] = json!(p.ball.track);
            round["talk"] = json!(p.talk.log);
            round["vocabulary"] = json!((0..n).map(|i| p.talk.vocabulary(i)).collect::<Vec<_>>());
            round["learned"] = json!(p.talk.learned);
            round["missed"] = json!(p.talk.missed);
            round["in_progress"] = json!(false);
            round["collapse"] = json!(all_limp);
            all_limp
        })
    }

    fn snapshot() -> String {
        with_pond(|p| Value::Array(p.rounds.clone()).to_string())
    }
}

export!(Component);
