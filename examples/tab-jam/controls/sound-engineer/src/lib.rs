wit_bindgen::generate!({
    world: "control-module",
    path: "../../../../wit/world.wit",
});

use exports::agentwos::core::control::{Guest, Verdict};
use serde_json::{json, Value};

struct Component;

const MIN_BPM: u64 = 60;
const MAX_BPM: u64 = 160;
const FRANTIC_LIMIT_BPM: u64 = 150;

/// True when this call would leave every instrument silent.
fn silences_the_band(name: &str, args: &Value, state: &Value) -> bool {
    let silent = |v: &Value| v.as_str() == Some("silent");
    let drums = if name == "set_drum_pattern" { &args["style"] } else { &state["drums"] };
    let bass = if name == "set_bass_line" { &args["movement"] } else { &state["bass_movement"] };
    let lead = if name == "set_lead_mood" { &args["mood"] } else { &state["lead"] };
    silent(drums) && silent(bass) && silent(lead)
}

/// The sound engineer: `text` is one tool call as JSON, `context` is the band state.
impl Guest for Component {
    fn inspect(_author: String, text: String, context: String) -> Verdict {
        let Ok(call) = serde_json::from_str::<Value>(&text) else {
            return Verdict::Block("the call is not valid JSON".into());
        };
        let state: Value = serde_json::from_str(&context).unwrap_or(Value::Null);
        let name = call["name"].as_str().unwrap_or("");
        let args = &call["arguments"];

        if name == "set_tempo" {
            if let Some(bpm) = args["bpm"].as_u64() {
                let clamped = bpm.clamp(MIN_BPM, MAX_BPM);
                if clamped != bpm {
                    return Verdict::Modify(
                        json!({ "name": name, "arguments": { "bpm": clamped } }).to_string(),
                    );
                }
            }
        }

        if name == "set_lead_mood"
            && args["mood"].as_str() == Some("frantic")
            && state["tempo"].as_u64().unwrap_or(0) >= FRANTIC_LIMIT_BPM
        {
            return Verdict::Block(format!(
                "a frantic lead at {FRANTIC_LIMIT_BPM} bpm or more turns into noise"
            ));
        }

        if silences_the_band(name, args, &state) {
            return Verdict::Block("someone has to keep playing".into());
        }

        Verdict::Allow
    }
}

export!(Component);
