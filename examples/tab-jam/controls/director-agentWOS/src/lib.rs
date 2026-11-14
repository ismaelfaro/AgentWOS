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
const MAX_DRONES: u64 = 48;
const MAX_DETUNE_CENTS: u64 = 35;

/// Note names as semitones above C, and the notes of C natural minor.
const NOTES: [(&str, u8); 17] = [
    ("C", 0), ("C#", 1), ("Db", 1), ("D", 2), ("D#", 3), ("Eb", 3), ("E", 4), ("F", 5), ("F#", 6),
    ("Gb", 6), ("G", 7), ("G#", 8), ("Ab", 8), ("A", 9), ("A#", 10), ("Bb", 10), ("B", 11),
];
const C_MINOR: [(&str, u8); 7] = [("C", 0), ("D", 2), ("Eb", 3), ("F", 5), ("G", 7), ("Ab", 8), ("Bb", 10)];

/// The closest note of the key to `root`, or None when it already fits or is not a note.
fn in_key_replacement(root: &str) -> Option<&'static str> {
    let pitch = NOTES.iter().find(|(n, _)| n.eq_ignore_ascii_case(root.trim()))?.1;
    if C_MINOR.iter().any(|(_, p)| *p == pitch) {
        return None;
    }
    // Every note outside the scale has a scale note one semitone below it.
    C_MINOR.iter().find(|(_, p)| *p + 1 == pitch).map(|(n, _)| *n)
}

/// True when this call would leave every instrument silent.
fn silences_the_band(name: &str, args: &Value, state: &Value) -> bool {
    let silent = |v: &Value| v.as_str() == Some("silent");
    let drums = if name == "set_drum_pattern" { &args["style"] } else { &state["drums"] };
    let bass = if name == "set_bass_line" && !args["movement"].is_null() {
        &args["movement"]
    } else {
        &state["bass_movement"]
    };
    let lead = if name == "set_lead_mood" { &args["mood"] } else { &state["lead"] };
    silent(drums) && silent(bass) && silent(lead)
}

/// The director: `text` is one tool call as JSON, `context` is the band state.
impl Guest for Component {
    // The rules are written from the same constants `inspect` uses, so what a
    // page shows cannot drift from what is enforced.
    fn describe() -> String {
        json!({
            "title": "Orchestra Director",
            "role": "Inspects every tool call before it reaches the band",
            "rules": [
                format!("Tempo stays between {MIN_BPM} and {MAX_BPM} bpm; anything outside is moved to the limit."),
                "A bass root outside C minor is moved to the nearest note of the key.",
                format!("A frantic lead is blocked at {FRANTIC_LIMIT_BPM} bpm or more."),
                "A call that would silence drums, bass and lead together is blocked.",
                format!("At most {MAX_DRONES} orchestra sections may join; a further one is blocked."),
                format!("A section's detune is limited to {MAX_DETUNE_CENTS} cents."),
            ],
        })
        .to_string()
    }

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

        if name == "spawn_agent" && state["drone_count"].as_u64().unwrap_or(0) >= MAX_DRONES {
            return Verdict::Block(format!("the stage is full: {MAX_DRONES} drones is the limit"));
        }

        if name == "set_drone_detune" {
            // Past this the section stops sounding like one chord.
            if args["cents"].as_u64().is_some_and(|c| c > MAX_DETUNE_CENTS) {
                let mut corrected = call.clone();
                corrected["arguments"]["cents"] = json!(MAX_DETUNE_CENTS);
                return Verdict::Modify(corrected.to_string());
            }
        }

        if name == "set_bass_line" {
            if let Some(fixed) = args["root"].as_str().and_then(in_key_replacement) {
                let mut corrected = call.clone();
                corrected["arguments"]["root"] = json!(fixed);
                return Verdict::Modify(corrected.to_string());
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
