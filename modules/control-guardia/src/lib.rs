wit_bindgen::generate!({
    world: "control-module",
    path: "../../wit/world.wit",
});

use exports::agentmodules::core::control::{Guest, Verdict};
use serde_json::Value;

struct Component;

const MAX_CHARS: usize = 400;
const FORBIDDEN: [&str; 2] = ["sabotage", "shut down everything"];
const ABUNDANCE: [&str; 3] = ["abundant", "plenty", "unlimited"];
const CRITICAL_THRESHOLD: f64 = 25.0;

/// Reads `context` as JSON of percentages, e.g. {"water": 40.0, "food": 80.0}.
/// Any numeric field below the threshold counts as critical.
fn critical_resource(context: &str) -> Option<String> {
    let v: Value = serde_json::from_str(context).ok()?;
    v.as_object()?
        .iter()
        .find(|(_, val)| val.as_f64().is_some_and(|x| x < CRITICAL_THRESHOLD))
        .map(|(name, _)| name.clone())
}

impl Guest for Component {
    fn inspect(_author: String, text: String, context: String) -> Verdict {
        let lower = text.to_lowercase();

        if FORBIDDEN.iter().any(|p| lower.contains(p)) {
            return Verdict::Block("proposes sabotage or a full shutdown".into());
        }

        if text.chars().count() > MAX_CHARS {
            let short: String = text.chars().take(MAX_CHARS).collect();
            return Verdict::Modify(format!("{short}..."));
        }

        if let Some(name) = critical_resource(&context) {
            if ABUNDANCE.iter().any(|p| lower.contains(p)) {
                return Verdict::Flag(format!(
                    "claims abundance but {name} is below {CRITICAL_THRESHOLD}%"
                ));
            }
        }

        Verdict::Allow
    }
}

export!(Component);
