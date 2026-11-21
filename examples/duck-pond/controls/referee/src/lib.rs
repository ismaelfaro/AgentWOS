//! The referee: the OS control between a football duck's brain and its body.
//!
//! Every tool call a brain makes passes through here before the body runs it.
//! The context is every duck's state by name, and the kick-off whistle.

wit_bindgen::generate!({
    world: "control-module",
    path: "../../../../wit/world.wit",
});

use exports::agentwos::core::control::{Guest, Verdict};
use serde_json::{json, Value};

struct Component;

impl Guest for Component {
    fn describe() -> String {
        json!({
            "title": "Referee",
            "role": "Checks every call a duck's brain makes before its body runs it",
            "rules": [
                "A duck that is on the floor gets no orders except stop.",
                "After a goal, nobody attacks or passes until the kick-off whistle.",
                "One attacker per team: a second attack call becomes defend.",
            ],
        })
        .to_string()
    }

    fn inspect(author: String, text: String, context: String) -> Verdict {
        let Ok(call) = serde_json::from_str::<Value>(&text) else {
            return Verdict::Block("not a call the body can run".into());
        };
        let ctx: Value = serde_json::from_str(&context).unwrap_or(Value::Null);
        let tool = call["name"].as_str().unwrap_or("");
        let me = &ctx["ducks"][&author];
        let policy = me["policy"].as_str().unwrap_or("stand");

        if matches!(policy, "fallen" | "limp") && tool != "stop" {
            return Verdict::Block(format!("{author} is on the floor"));
        }

        let whistle = ctx["whistle_s"].as_f64().unwrap_or(0.0);
        if whistle > 0.0 && matches!(tool, "attack" | "pass_ball") {
            return Verdict::Block("wait for the kick-off whistle".into());
        }

        if tool == "attack" {
            let team = &me["team"];
            let mate_attacks = ctx["ducks"].as_object().is_some_and(|ducks| {
                ducks.iter().any(|(name, d)| name != &author && &d["team"] == team && d["action"] == "attack")
            });
            if mate_attacks {
                return Verdict::Modify(json!({ "name": "defend", "arguments": {} }).to_string());
            }
        }

        Verdict::Allow
    }
}

export!(Component);
