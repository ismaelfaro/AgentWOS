//! Duck soccer: two teams of two MicroDucks on a small pitch.
//!
//! The bodies are the pond's (`body.rs` is shared): the same gait, ToF reflex,
//! falls and get-up. The brain of each duck is an `actor` with a small
//! tool-calling model, which turns one plain sentence into one of four calls:
//! attack, defend, pass_ball or stop. The sentence comes from a person coaching
//! ("Pip attack", "Sun defend") or from the duck itself: when the game changes,
//! its body says what it wants to do, and the same brain acts on it.
//!
//! What a call means in legs is this file's job: where to stand, how to get
//! behind the ball, when to kick. The model picks the role, not the footwork.

wit_bindgen::generate!({
    world: "stage-module",
    path: "../../../../wit/world.wit",
});

// The pond's scenario uses parts of the body that this stage does not.
#[allow(dead_code)]
#[path = "../../../scenario/src/body.rs"]
mod body;

use std::cell::RefCell;

use body::{Ball, Decision, Duck, Policy, Room, DT};
use exports::agentwos::core::stage::Guest;
use serde_json::{json, Value};

struct Component;

const W: f64 = 3.4;
const H: f64 = 2.4;
/// Half the width of a goal mouth, centred on each end wall.
const GOAL_HALF: f64 = 0.5;
const TEAMS: [&str; 2] = ["Sky", "Sun"];

struct Player {
    name: &'static str,
    colour: &'static str,
    team: usize,
    home: (f64, f64),
}

/// In the order of the colourways the page draws: blue, grey, white, purple.
const PLAYERS: [Player; 4] = [
    Player { name: "Pip", colour: "blue", team: 0, home: (1.15, 1.2) },
    Player { name: "Momo", colour: "grey", team: 1, home: (2.25, 1.2) },
    Player { name: "Nilo", colour: "white", team: 1, home: (2.9, 1.2) },
    Player { name: "Lila", colour: "purple", team: 0, home: (0.5, 1.2) },
];

const KICKOFF_WHISTLE_S: f64 = 2.0;
/// A duck that a person coached keeps that order this long before its own urges return.
const COACH_HOLD_S: f64 = 10.0;
const TIRED: f64 = 0.15;
const RECOVERED: f64 = 0.4;
/// A ball that has not moved for this long near a wall is dropped back in play.
const STUCK_S: f64 = 5.0;
/// A duck that cannot get up is set on its feet by the referee after this long.
const HELP_AFTER_S: f64 = 6.0;
/// Match fitness: energy a duck gets back per second while it plays. The pond's
/// ducks tire in a minute and rest for another, which makes for half a match.
const STAMINA: f64 = 1.0 / 150.0;

struct Mind {
    action: &'static str,
    /// The call this duck's own urge asked for; a different call is a coach's.
    pending: Option<&'static str>,
    hold_s: f64,
    think_s: f64,
    next_think_s: f64,
    limp_s: f64,
    last_event: (String, f64),
}

struct Match {
    room: Room,
    ducks: Vec<Duck>,
    minds: Vec<Mind>,
    ball: Ball,
    score: [u32; 2],
    whistle_s: f64,
    still: ((f64, f64), f64),
    t: f64,
}

thread_local! {
    static MATCH: RefCell<Option<Match>> = const { RefCell::new(None) };
}

fn with_match<R>(f: impl FnOnce(&mut Match) -> R) -> R {
    MATCH.with(|m| f(m.borrow_mut().as_mut().expect("setup must run first")))
}

fn function(name: &str, description: &str) -> Value {
    json!({ "type": "function", "function": {
        "name": name, "description": description,
        "parameters": { "type": "object", "properties": {}, "required": [] },
    }})
}

fn tools() -> Value {
    json!([
        function("attack", "Attack: chase the ball and shoot at the goal."),
        function("defend", "Defend: go back and guard your own goal."),
        function("pass_ball", "Pass the ball to your teammate."),
        function("stop", "Stop and stand still."),
    ])
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
}

fn towards(from: (f64, f64), to: (f64, f64), len: f64) -> (f64, f64) {
    let d = dist(from, to).max(1e-6);
    (from.0 + (to.0 - from.0) / d * len, from.1 + (to.1 - from.1) / d * len)
}

/// The goal a team defends: Sky the left wall, Sun the right.
fn own_goal(team: usize) -> (f64, f64) {
    (if team == 0 { 0.0 } else { W }, H / 2.0)
}

fn pos(d: &Duck) -> (f64, f64) {
    (d.x, d.y)
}

fn fresh_duck(i: usize, room: &Room, seed: u64, energy: f64) -> Duck {
    let p = &PLAYERS[i];
    let heading = if p.team == 0 { 0.0 } else { std::f64::consts::PI };
    let mut d = Duck::new(p.name, p.home.0, p.home.1, heading, seed, room);
    d.energy = energy;
    d
}

fn decide(state: &str, goal: Option<(f64, f64)>) -> Decision {
    Decision { state: state.into(), goal, thought: String::new() }
}

/// Getting the ball to `target`: walk to a spot behind the ball on the line to
/// the target, then walk through the ball. The body kicks along its heading.
fn kick_towards(me: (f64, f64), ball: (f64, f64), target: (f64, f64)) -> Decision {
    let behind = towards(ball, target, -0.3);
    let behind = (behind.0.clamp(0.14, W - 0.14), behind.1.clamp(0.14, H - 0.14));
    // Lined up when the ball lies between the duck and the target.
    let to_ball = towards(me, ball, 1.0);
    let to_target = towards(ball, target, 1.0);
    let lined_up = (to_ball.0 - me.0) * (to_target.0 - ball.0) + (to_ball.1 - me.1) * (to_target.1 - ball.1) > 0.75;
    // Close enough that the body walks the last steps slowly: a bump at the ball topples nobody.
    if dist(me, behind) < 0.27 || (lined_up && dist(me, ball) < 0.42) {
        decide("ball", None)
    } else {
        decide("wander", Some(behind))
    }
}

/// What a call means for the legs right now.
fn footwork(m: &Match, i: usize) -> Decision {
    let team = PLAYERS[i].team;
    let me = pos(&m.ducks[i]);
    let ball = (m.ball.x, m.ball.y);
    let their_goal = own_goal(1 - team);
    match m.minds[i].action {
        "attack" => kick_towards(me, ball, their_goal),
        "pass_ball" => {
            let mate = (0..PLAYERS.len()).find(|&j| j != i && PLAYERS[j].team == team).unwrap_or(i);
            kick_towards(me, ball, pos(&m.ducks[mate]))
        }
        "defend" => {
            let goal = own_goal(team);
            if dist(ball, goal) < 0.9 {
                // The ball is at the goal mouth: clear it.
                kick_towards(me, ball, their_goal)
            } else {
                // Out at the edge of the goal area, not on the line: a keeper on the line
                // stops every shot of a 2-against-2 game.
                let spot = towards(goal, ball, 0.7);
                if dist(me, spot) < 0.25 {
                    decide("turn", Some(ball))
                } else {
                    decide("wander", Some(spot))
                }
            }
        }
        _ => decide("chill", None),
    }
}

/// What this duck wants to do given the game, and why. None when nothing changes.
fn urge(m: &mut Match, i: usize) -> Option<(&'static str, String, String)> {
    let name = PLAYERS[i].name;
    let team = PLAYERS[i].team;
    let ball = (m.ball.x, m.ball.y);
    let me = pos(&m.ducks[i]);
    let energy = m.ducks[i].energy;
    let current = m.minds[i].action;

    let (want, why): (&'static str, String) = if energy < TIRED || (current == "stop" && energy < RECOVERED && m.t > 5.0) {
        ("stop", format!("out of breath at {:.0}% energy", energy * 100.0))
    } else {
        let mate = (0..PLAYERS.len()).find(|&j| j != i && PLAYERS[j].team == team).unwrap_or(i);
        let mate_up = !matches!(m.ducks[mate].policy, Policy::Fallen | Policy::Limp);
        let closest = !mate_up || dist(me, ball) <= dist(pos(&m.ducks[mate]), ball);
        if closest {
            let pressed = (0..PLAYERS.len()).any(|j| PLAYERS[j].team != team && dist(pos(&m.ducks[j]), ball) < 0.45);
            if pressed && mate_up && dist(me, ball) < 0.5 && m.ducks[i].rng.unit() < 0.35 {
                ("pass_ball", "an opponent is on the ball".into())
            } else {
                ("attack", "closest to the ball".into())
            }
        } else {
            ("defend", format!("{} is closer to the ball", PLAYERS[mate].name))
        }
    };
    if want == current {
        return None;
    }
    let text = match want {
        "attack" => format!("{name} attack"),
        "defend" => format!("{name} defend the goal"),
        "pass_ball" => format!("{name} pass the ball"),
        _ => format!("{name} stop"),
    };
    Some((want, text, why))
}

fn kick_off(m: &mut Match) {
    for i in 0..PLAYERS.len() {
        let energy = m.ducks[i].energy;
        // A fresh body at the home spot: its timers and reflex state start clean.
        m.ducks[i] = fresh_duck(i, &m.room, (m.t * 1000.0) as u64 + 17 * i as u64 + 1, energy);
        m.minds[i].action = "stop";
        m.minds[i].pending = None;
        m.minds[i].hold_s = 0.0;
        m.minds[i].think_s = 0.0;
        m.minds[i].limp_s = 0.0;
    }
    m.ball = Ball::new(W / 2.0, H / 2.0);
    m.whistle_s = KICKOFF_WHISTLE_S;
    m.still = ((W / 2.0, H / 2.0), 0.0);
}

fn duck_state(m: &Match, i: usize) -> Value {
    let d = &m.ducks[i];
    json!({
        "agent": d.name,
        "team": TEAMS[PLAYERS[i].team],
        "x": d.x, "y": d.y, "heading": d.heading,
        "energy": d.energy,
        "policy": d.policy.name(),
        "action": m.minds[i].action,
        "walking": d.policy == Policy::Walk,
    })
}

fn frame(m: &Match, events: Vec<Value>, urges: Vec<Value>) -> Value {
    json!({
        "t": m.t,
        "ducks": (0..m.ducks.len()).map(|i| duck_state(m, i)).collect::<Vec<_>>(),
        "ball": [m.ball.x, m.ball.y],
        "score": { "Sky": m.score[0], "Sun": m.score[1] },
        "whistle_s": m.whistle_s,
        "events": events,
        "urges": urges,
    })
}

impl Guest for Component {
    fn setup(_config: String) -> String {
        let room = Room { w: W, h: H, obstacles: Vec::new(), cushion: (W / 2.0, H / 2.0) };
        let ducks: Vec<Duck> = (0..PLAYERS.len()).map(|i| fresh_duck(i, &room, 31 + i as u64, 0.9)).collect();
        let minds = (0..PLAYERS.len())
            .map(|i| Mind {
                action: "stop",
                pending: None,
                hold_s: 0.0,
                think_s: 0.0,
                // Staggered, so the four brains do not all speak in the same tick.
                next_think_s: 0.6 + i as f64 * 0.5,
                limp_s: 0.0,
                last_event: (String::new(), -10.0),
            })
            .collect();
        let m = Match { room, ducks, minds, ball: Ball::new(W / 2.0, H / 2.0), score: [0, 0], whistle_s: 1.0, still: ((W / 2.0, H / 2.0), 0.0), t: 0.0 };
        let out = json!({
            "agents": PLAYERS.iter().map(|p| json!({
                "agent": p.name, "role": format!("{} team", TEAMS[p.team]), "team": TEAMS[p.team], "colour": p.colour,
            })).collect::<Vec<_>>(),
            "pitch": { "w": W, "h": H, "goal_half": GOAL_HALF },
            "teams": [{ "name": "Sky", "defends": "left" }, { "name": "Sun", "defends": "right" }],
            "tick_s": DT,
            "suggestions": [
                "Pip attack",
                "Lila defend the goal",
                "Momo pass the ball",
                "Sky attack",
                "Sun defend the goal",
                "Everyone stop",
            ],
        });
        MATCH.with(|slot| *slot.borrow_mut() = Some(m));
        out.to_string()
    }

    fn brief(agent: u32) -> String {
        let p = &PLAYERS[agent as usize];
        json!({
            // The same words for every duck: the engine reuses the prompt it prepared
            // while system and tools do not change. The aliases keep each duck to its own requests.
            "system": "You are a small robot duck playing football. Act on the request.",
            "aliases": [p.name.to_lowercase(), p.colour, TEAMS[p.team].to_lowercase(), "everyone", "all", "ducks"],
            // The model reads the order without the name: a name in front makes it guess wrong.
            "strip_aliases": true,
            "tools": tools(),
        })
        .to_string()
    }

    /// Every duck's state by name, and the whistle: the referee reads its author's entry.
    fn context() -> String {
        with_match(|m| {
            let mut ducks = serde_json::Map::new();
            for i in 0..m.ducks.len() {
                ducks.insert(
                    PLAYERS[i].name.to_string(),
                    json!({
                        "team": TEAMS[PLAYERS[i].team],
                        "policy": m.ducks[i].policy.name(),
                        "energy": m.ducks[i].energy,
                        "action": m.minds[i].action,
                    }),
                );
            }
            json!({ "ducks": ducks, "whistle_s": m.whistle_s }).to_string()
        })
    }

    fn apply(agent: u32, call: String) -> Result<String, String> {
        let call: Value = serde_json::from_str(&call).map_err(|e| format!("call is not JSON: {e}"))?;
        let tool = call["name"].as_str().unwrap_or("");
        with_match(|m| {
            let i = agent as usize;
            let name = PLAYERS[i].name;
            let (action, said): (&'static str, String) = match tool {
                "attack" => ("attack", format!("{name} goes for the ball")),
                "defend" => ("defend", format!("{name} drops back to the goal")),
                "pass_ball" => ("pass_ball", format!("{name} looks for a pass")),
                "stop" => ("stop", format!("{name} stops")),
                other => return Err(format!("unknown tool '{other}'")),
            };
            let mind = &mut m.minds[i];
            // A call the duck's own urge did not ask for came from the coach: it holds.
            if mind.pending != Some(action) {
                mind.hold_s = COACH_HOLD_S;
            }
            mind.pending = None;
            mind.action = action;
            mind.think_s = 0.0;
            Ok(said)
        })
    }

    fn spawn(_kind: String) -> Result<String, String> {
        Err("a match is two against two".into())
    }

    /// Advances the match one tick and returns the frame.
    fn render(_tick: u32) -> String {
        with_match(|m| {
            let n = m.ducks.len();
            let mut events = Vec::new();
            let mut urges = Vec::new();

            for i in 0..n {
                let decision = footwork(m, i);
                // A new decision of the same kind keeps the body's own state (its walk does not restart).
                // Going for the ball, the reflex is off, as in MicroDuck's own soccer.
                m.ducks[i].reflex = decision.state != "ball";
                m.ducks[i].decision = decision;
            }
            let step = (m.t / DT).round() as usize;
            for k in 0..n {
                body::step(&m.room, &mut m.ducks, &mut m.ball, (k + step) % n);
            }
            body::step_ball(&m.room, &mut m.ball);
            m.t += DT;
            m.whistle_s = (m.whistle_s - DT).max(0.0);

            for i in 0..n {
                body::after_step(&m.room, &mut m.ducks[i], false);
                m.ducks[i].energy = (m.ducks[i].energy + STAMINA * DT).min(1.0);
                for e in std::mem::take(&mut m.ducks[i].events) {
                    // Arriving at a spot is footwork, not news.
                    if e == "arrived at its goal" {
                        continue;
                    }
                    if e == "kicked the ball" && m.minds[i].action == "pass_ball" {
                        // A pass is one kick: after it the duck thinks again.
                        m.minds[i].action = "stop";
                        m.minds[i].think_s = m.minds[i].next_think_s;
                    }
                    let mind = &mut m.minds[i];
                    if mind.last_event.0 != e || m.t - mind.last_event.1 > 3.0 {
                        mind.last_event = (e.clone(), m.t);
                        events.push(json!({ "agent": PLAYERS[i].name, "text": e }));
                    }
                }

                // The referee sets a duck that cannot get up back on its feet.
                if m.ducks[i].policy == Policy::Limp {
                    m.minds[i].limp_s += DT;
                    if m.minds[i].limp_s >= HELP_AFTER_S {
                        let (x, y, heading, energy) = (m.ducks[i].x, m.ducks[i].y, m.ducks[i].heading, m.ducks[i].energy);
                        m.ducks[i] = fresh_duck(i, &m.room, (m.t * 1000.0) as u64 + i as u64, energy);
                        m.ducks[i].x = x;
                        m.ducks[i].y = y;
                        m.ducks[i].heading = heading;
                        m.minds[i].limp_s = 0.0;
                        events.push(json!({ "agent": PLAYERS[i].name, "text": "the referee helps it back up" }));
                    }
                }
            }

            // A goal: the ball reaches an end wall inside the goal mouth.
            let in_mouth = (m.ball.y - H / 2.0).abs() < GOAL_HALF;
            let scorer = if in_mouth && m.ball.x < 0.12 {
                Some(1)
            } else if in_mouth && m.ball.x > W - 0.12 {
                Some(0)
            } else {
                None
            };
            if let Some(team) = scorer {
                m.score[team] += 1;
                events.push(json!({ "agent": TEAMS[team], "text": format!("GOAL for {}! {} - {}", TEAMS[team], m.score[0], m.score[1]), "goal": TEAMS[team] }));
                kick_off(m);
                return frame(m, events, urges).to_string();
            }

            // A ball nobody can get out of a corner is dropped back in play.
            let ball = (m.ball.x, m.ball.y);
            if dist(ball, m.still.0) > 0.03 {
                m.still = (ball, 0.0);
            } else {
                m.still.1 += DT;
                let near_wall = ball.0 < 0.2 || ball.0 > W - 0.2 || ball.1 < 0.2 || ball.1 > H - 0.2;
                if near_wall && m.still.1 >= STUCK_S {
                    m.ball = Ball::new(W / 2.0, H / 2.0);
                    m.still = ((W / 2.0, H / 2.0), 0.0);
                    events.push(json!({ "agent": "Referee", "text": "ball stuck at the wall: dropped at the centre" }));
                }
            }

            // Urges: each duck looks at the game now and then, and speaks when it wants a new role.
            for i in 0..n {
                let down = matches!(m.ducks[i].policy, Policy::Fallen | Policy::Limp);
                let mind = &mut m.minds[i];
                mind.hold_s = (mind.hold_s - DT).max(0.0);
                mind.think_s += DT;
                if down || m.whistle_s > 0.0 || mind.hold_s > 0.0 || mind.think_s < mind.next_think_s {
                    continue;
                }
                mind.think_s = 0.0;
                mind.next_think_s = 2.0 + m.ducks[i].rng.unit() * 1.5;
                if let Some((want, text, why)) = urge(m, i) {
                    m.minds[i].pending = Some(want);
                    urges.push(json!({ "agent": i, "text": text, "why": why }));
                }
            }

            frame(m, events, urges).to_string()
        })
    }

    fn state() -> String {
        with_match(|m| frame(m, Vec::new(), Vec::new()).to_string())
    }
}

export!(Component);
