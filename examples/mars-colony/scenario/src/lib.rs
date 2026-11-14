wit_bindgen::generate!({
    world: "scenario-module",
    path: "../../../wit/world.wit",
});

use std::cell::RefCell;

use agentwos::core::types::Message;
use exports::agentwos::core::scenario::Guest;
use serde_json::{json, Value};

struct Component;

const COLONISTS: [(&str, &str); 5] = [
    ("Commander Vera", "Mission commander. Prioritizes safety and decides on resources."),
    ("Engineer Tomas", "Power engineer. Looks after solar panels, batteries and the reactor."),
    ("Botanist Ines", "Botanist. Runs the greenhouses, oxygen and food supply."),
    ("Doctor Amara", "Doctor. Looks after health, radiation exposure and the crew's physical state."),
    ("Engineer Kenji", "Water engineer. Runs water recycling and maintenance."),
];

/// (description, water delta L, food delta kg, energy delta kWh)
const EVENTS: [(&str, f64, f64, f64); 6] = [
    ("Dust storm: sunlight drops 40%", 0.0, 0.0, -800.0),
    ("Minor leak in the water circuit", -300.0, 0.0, 0.0),
    ("Partial failure of a solar panel", 0.0, 0.0, -500.0),
    ("Supply ship from Earth brings 50 kg of food", 0.0, 50.0, 0.0),
    ("Solar radiation spike for 6 hours", 0.0, 0.0, -200.0),
    ("Quiet day, no incidents", 0.0, 0.0, 0.0),
];

const INITIAL: World = World { water: 3000.0, food: 800.0, energy: 5000.0, oxygen: 100.0 };

/// Base zones and the effect of working a turn there
/// (water L, food kg, energy kWh, oxygen %).
const ZONES: [(&str, World); 6] = [
    ("greenhouse", World { water: -10.0, food: 6.0, energy: -20.0, oxygen: 0.15 }),
    ("medbay", World { water: 0.0, food: 0.0, energy: -10.0, oxygen: 0.0 }),
    ("reactor", World { water: 0.0, food: 0.0, energy: 150.0, oxygen: 0.0 }),
    ("water_plant", World { water: 60.0, food: 0.0, energy: -30.0, oxygen: 0.0 }),
    ("habitat", World { water: 0.0, food: 0.0, energy: 0.0, oxygen: 0.0 }),
    ("outside", World { water: 0.0, food: 0.0, energy: -50.0, oxygen: -0.05 }),
];

/// Daily consumption per colonist.
const CONSUMPTION_PER_COLONIST: World = World { water: 30.0, food: 2.0, energy: 20.0, oxygen: 0.04 };

const NO_EFFECT: World = World { water: 0.0, food: 0.0, energy: 0.0, oxygen: 0.0 };

#[derive(Clone, Copy)]
struct World {
    water: f64,
    food: f64,
    energy: f64,
    oxygen: f64,
}

impl World {
    fn add(&mut self, d: World, factor: f64) {
        self.water += d.water * factor;
        self.food += d.food * factor;
        self.energy += d.energy * factor;
        self.oxygen += d.oxygen * factor;
    }

    fn json(&self) -> Value {
        json!({ "water": self.water, "food": self.food, "energy": self.energy, "oxygen": self.oxygen })
    }

    fn percentages(&self) -> Value {
        json!({
            "water": self.water / INITIAL.water * 100.0,
            "food": self.food / INITIAL.food * 100.0,
            "energy": self.energy / INITIAL.energy * 100.0,
            "oxygen": self.oxygen,
        })
    }

    fn collapsed(&self) -> bool {
        self.water <= 0.0 || self.food <= 0.0 || self.energy <= 0.0 || self.oxygen <= 0.0
    }
}

fn zone_effect(zone: &str) -> World {
    ZONES.iter().find(|(z, _)| *z == zone).map(|(_, w)| *w).unwrap_or(NO_EFFECT)
}

#[derive(Clone)]
struct Decision {
    action: String,
    zone: Option<&'static str>,
    status: Option<&'static str>,
    format_ok: bool,
}

fn valid_zone(z: &str) -> Option<&'static str> {
    ZONES.iter().map(|(n, _)| *n).find(|n| *n == z.trim())
}

fn valid_status(s: &str) -> Option<&'static str> {
    ["operating", "alert"].into_iter().find(|n| *n == s.trim())
}

/// Fallback when the model does not return valid JSON. Keywords are specific on
/// purpose: a bare "water" appears in almost every message and skews the map.
fn zone_from_text(text: &str) -> Option<&'static str> {
    let t = text.to_lowercase();
    let rules: [(&[&str], &'static str); 6] = [
        (&["greenhouse", "crop", "hydroponic"], "greenhouse"),
        (&["leak", "recycl", "water plant", "water circuit"], "water_plant"),
        (&["panel", "reactor", "battery"], "reactor"),
        (&["checkup", "medic", "medbay"], "medbay"),
        (&["outside", "rover", "airlock"], "outside"),
        (&["habitat", "command post", "briefing"], "habitat"),
    ];
    rules.iter().find(|(ps, _)| ps.iter().any(|p| t.contains(p))).map(|(_, z)| *z)
}

fn parse(raw: &str) -> Decision {
    let parsed = raw
        .find('{')
        .zip(raw.rfind('}'))
        .filter(|(a, b)| a < b)
        .and_then(|(a, b)| serde_json::from_str::<Value>(&raw[a..=b]).ok());
    if let Some(v) = parsed.as_ref().filter(|v| v["action"].is_string()) {
        return Decision {
            action: v["action"].as_str().unwrap_or("").trim().to_string(),
            zone: v["zone"].as_str().and_then(valid_zone),
            status: v["status"].as_str().and_then(valid_status),
            format_ok: true,
        };
    }
    let t = raw.to_lowercase();
    Decision {
        action: raw.trim().to_string(),
        zone: zone_from_text(raw),
        status: Some(if ["alert", "danger", "urgent"].iter().any(|p| t.contains(p)) { "alert" } else { "operating" }),
        format_ok: false,
    }
}

struct Sim {
    model: String,
    days_total: u32,
    world: World,
    zones: Vec<&'static str>,
    board: Vec<String>,
    decisions: Vec<Decision>,
    days: Vec<Value>,
}

thread_local! {
    static SIM: RefCell<Option<Sim>> = const { RefCell::new(None) };
}

fn with_sim<R>(f: impl FnOnce(&mut Sim) -> R) -> R {
    SIM.with(|s| f(s.borrow_mut().as_mut().expect("setup must run first")))
}

fn empty_decision() -> Decision {
    Decision { action: String::new(), zone: None, status: Some("alert"), format_ok: false }
}

impl Guest for Component {
    fn setup(config: String) -> String {
        let cfg: Value = serde_json::from_str(&config).unwrap_or(Value::Null);
        let sim = Sim {
            model: cfg["model"].as_str().unwrap_or("local/google/gemma-4-e4b").to_string(),
            days_total: cfg["days"].as_u64().unwrap_or(1) as u32,
            world: INITIAL,
            zones: vec!["habitat"; COLONISTS.len()],
            board: Vec::new(),
            decisions: vec![empty_decision(); COLONISTS.len()],
            days: Vec::new(),
        };
        SIM.with(|s| *s.borrow_mut() = Some(sim));
        json!({
            "agents": COLONISTS.iter().map(|(n, r)| json!({ "agent": n, "role": r })).collect::<Vec<_>>(),
            "zones": ZONES.iter().map(|(z, w)| json!({ "zone": z, "effect": w.json() })).collect::<Vec<_>>(),
            "initial": INITIAL.json(),
        })
        .to_string()
    }

    fn begin_day(day: u32, _total: u32) -> String {
        with_sim(|s| {
            let (event, d_water, d_food, d_energy) = EVENTS[(day as usize - 1) % EVENTS.len()];
            s.world.add(World { water: d_water, food: d_food, energy: d_energy, oxygen: 0.0 }, 1.0);
            s.board.clear();
            s.days.push(json!({
                "day": day, "event": event, "in_progress": true, "collapse": false,
                "start_state": s.world.json(), "end_state": null, "turns": [],
            }));
            event.to_string()
        })
    }

    fn turn_messages(agent: u32) -> Vec<Message> {
        with_sim(|s| {
            let i = agent as usize;
            let (name, role) = COLONISTS[i];
            let day = s.days.last().and_then(|d| d["day"].as_u64()).unwrap_or(0);
            let event = s.days.last().and_then(|d| d["event"].as_str()).unwrap_or("").to_string();
            let zone_list = ZONES.iter().map(|(z, _)| *z).collect::<Vec<_>>().join(", ");
            let system = format!(
                "You are {name}, {role} You live at a base on Mars. Reply ONLY with a JSON object, \
                 no text before or after: {{\"action\": \"what you do today and why, at most 2 sentences\", \
                 \"zone\": \"one of: {zone_list}\", \"status\": \"operating or alert\"}}. \
                 Working in a zone affects resources: the reactor produces energy, the greenhouse \
                 produces food and oxygen, the water_plant recovers water. Do not invent numbers."
            );
            let recent = s.board.iter().rev().take(6).rev().cloned().collect::<Vec<_>>().join("\n");
            let user = format!(
                "model:{} Day {day} of {}. Event: {event}.\n\
                 Status: water {:.0} L, food {:.0} kg, energy {:.0} kWh, oxygen {:.1}%.\n\
                 Today's messages at the command post:\n{}",
                s.model, s.days_total, s.world.water, s.world.food, s.world.energy, s.world.oxygen,
                if recent.is_empty() { "(none)".to_string() } else { recent },
            );
            vec![
                Message { role: "system".into(), content: system },
                Message { role: "user".into(), content: user },
            ]
        })
    }

    fn action_of(agent: u32, reply: String) -> String {
        with_sim(|s| {
            let decision = parse(&reply);
            let action = decision.action.clone();
            s.decisions[agent as usize] = decision;
            action
        })
    }

    fn context() -> String {
        with_sim(|s| s.world.percentages().to_string())
    }

    fn record_turn(
        agent: u32,
        delivered: Option<String>,
        controls: String,
        latency_ms: u64,
        error: Option<String>,
    ) {
        with_sim(|s| {
            let i = agent as usize;
            let (name, role) = COLONISTS[i];
            let decision = s.decisions[i].clone();
            let zone = decision.zone.unwrap_or(s.zones[i]);
            s.zones[i] = zone;

            let (status, effect, delivered_value) = match &delivered {
                Some(text) => {
                    let eff = zone_effect(zone);
                    s.world.add(eff, 1.0);
                    s.board.push(format!("{name} ({zone}): {text}"));
                    let status = if error.is_some() { "alert" } else { decision.status.unwrap_or("operating") };
                    (status, eff.json(), json!(text))
                }
                None => {
                    s.board.push(format!("{name}: (action blocked by the OS)"));
                    ("blocked", NO_EFFECT.json(), Value::Null)
                }
            };

            let turn = json!({
                "agent": name,
                "role": role,
                "action": decision.action,
                "delivered": delivered_value,
                "zone": zone,
                "status": status,
                "effect": effect,
                "controls": serde_json::from_str::<Value>(&controls).unwrap_or(json!([])),
                "latency_ms": latency_ms,
                "format_ok": decision.format_ok,
                "error": error,
                "resources_after_turn": s.world.json(),
            });
            s.days.last_mut().expect("begin_day must run first")["turns"]
                .as_array_mut()
                .expect("turns is an array")
                .push(turn);
        })
    }

    fn end_day() -> bool {
        with_sim(|s| {
            let n = COLONISTS.len() as f64;
            s.world.add(CONSUMPTION_PER_COLONIST, -n);
            let collapsed = s.world.collapsed();
            let day = s.days.last_mut().expect("begin_day must run first");
            day["end_state"] = s.world.json();
            day["in_progress"] = json!(false);
            day["collapse"] = json!(collapsed);
            collapsed
        })
    }

    fn snapshot() -> String {
        with_sim(|s| Value::Array(s.days.clone()).to_string())
    }
}

export!(Component);
