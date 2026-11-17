//! Duck safety: the OS control between a duck's brain and its body.
//!
//! Every decision a brain makes passes through here before the body runs it.
//! The rules are MicroDuck's (`docs/design/autonomous.md`): a duck that is down
//! belongs to the get-up, zoomies need clear ToF ahead, and below 0.2 energy a
//! duck has had enough.

wit_bindgen::generate!({
    world: "control-module",
    path: "../../../../wit/world.wit",
});

use exports::agentwos::core::control::{Guest, Verdict};
use serde_json::{json, Value};

struct Component;

const TIRED_ENERGY: f64 = 0.2;
const ZOOMIES_CLEAR_M: f64 = 0.45;
const MOVING: [&str; 4] = ["wander", "turn", "zoomies", "ball"];

impl Guest for Component {
    fn describe() -> String {
        json!({
            "title": "Duck safety",
            "role": "Checks every decision a duck's brain makes before its body runs it",
            "rules": [
                "A duck that is fallen or limp gets no new state: the get-up watch owns the body.",
                format!("Below {:.0}% energy, a moving state becomes a nap.", TIRED_ENERGY * 100.0),
                format!("Zoomies need clear ToF for {ZOOMIES_CLEAR_M} m ahead; otherwise they become a wander."),
                "A goal outside the room is flagged; the body clamps it to the walls.",
            ],
        })
        .to_string()
    }

    fn inspect(_author: String, text: String, context: String) -> Verdict {
        let Ok(mut decision) = serde_json::from_str::<Value>(&text) else {
            return Verdict::Block("not a decision the body can run".into());
        };
        let ctx: Value = serde_json::from_str(&context).unwrap_or(Value::Null);
        let state = decision["state"].as_str().unwrap_or("chill").to_string();
        let policy = ctx["policy"].as_str().unwrap_or("stand");
        let energy = ctx["energy"].as_f64().unwrap_or(1.0);

        if matches!(policy, "fallen" | "limp") && state != "chill" {
            return Verdict::Block(format!("the duck is {policy}: the get-up watch owns the body"));
        }

        if energy < TIRED_ENERGY && MOVING.contains(&state.as_str()) {
            decision["state"] = json!("nap");
            decision["goal"] = Value::Null;
            return Verdict::Modify(decision.to_string());
        }

        if state == "zoomies" {
            if let Some(ahead) = ctx["tof_ahead_m"].as_f64() {
                if ahead < ZOOMIES_CLEAR_M {
                    decision["state"] = json!("wander");
                    return Verdict::Modify(decision.to_string());
                }
            }
        }

        if let (Some(goal), Some(room)) = (decision["goal"].as_array(), ctx["room"].as_array()) {
            let (gx, gy) = (goal.first().and_then(Value::as_f64), goal.get(1).and_then(Value::as_f64));
            let (w, h) = (room.first().and_then(Value::as_f64), room.get(1).and_then(Value::as_f64));
            if let (Some(gx), Some(gy), Some(w), Some(h)) = (gx, gy, w, h) {
                if gx < 0.0 || gy < 0.0 || gx > w || gy > h {
                    return Verdict::Flag(format!("goal ({gx:.1}, {gy:.1}) is outside the room"));
                }
            }
        }

        Verdict::Allow
    }
}

export!(Component);
