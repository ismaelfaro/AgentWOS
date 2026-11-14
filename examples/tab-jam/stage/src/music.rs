//! Shared by every agent of the band: note names, scales, the bar grid, and
//! small helpers for tool declarations, argument checks and note events.

use serde_json::{json, Value};

pub const STEPS_PER_BAR: u32 = 16;
pub const KEY: &str = "C minor";

/// Every note name an agent accepts, as semitones above C. The director
/// control decides whether a note fits the key; the stage only needs a pitch.
pub const NOTES: [(&str, u8); 17] = [
    ("C", 0), ("C#", 1), ("Db", 1), ("D", 2), ("D#", 3), ("Eb", 3), ("E", 4), ("F", 5), ("F#", 6),
    ("Gb", 6), ("G", 7), ("G#", 8), ("Ab", 8), ("A", 9), ("A#", 10), ("Bb", 10), ("B", 11),
];
/// C minor pentatonic and C blues, as semitones above C.
pub const PENTATONIC: [u8; 5] = [0, 3, 5, 7, 10];
pub const BLUES: [u8; 6] = [0, 3, 5, 6, 7, 10];

/// One OpenAI-style tool declaration.
pub fn function(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "type": "function", "function": {
        "name": name, "description": description,
        "parameters": { "type": "object", "properties": properties, "required": required },
    }})
}

/// One note event of a bar. `step` and `len` are in sixteenths.
pub fn note(inst: &str, step: u32, len: u32, midi: u8, vel: u8) -> Value {
    json!({ "inst": inst, "step": step, "len": len, "midi": midi, "vel": vel })
}

/// Small deterministic generator so a bar always renders the same way.
pub fn rng(seed: u32) -> impl FnMut() -> u32 {
    let mut x = seed.wrapping_mul(2654435761).max(1);
    move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    }
}

/// Reads an argument that must be one of `options` ("half time" matches `half_time`).
pub fn pick(options: &[&'static str], value: &Value, field: &str) -> Result<&'static str, String> {
    let v = value.as_str().ok_or_else(|| format!("`{field}` must be a string"))?;
    let wanted = v.trim().to_lowercase().replace(' ', "_");
    options
        .iter()
        .copied()
        .find(|o| *o == wanted)
        .ok_or_else(|| format!("`{field}` must be one of {}, got '{v}'", options.join(", ")))
}

/// Reads an argument that must be a note name, in any case.
pub fn note_name(value: &Value) -> Result<&'static str, String> {
    let v = value.as_str().ok_or("`root` must be a note name")?;
    NOTES
        .iter()
        .map(|(n, _)| *n)
        .find(|n| n.eq_ignore_ascii_case(v.trim()))
        .ok_or_else(|| format!("`root` must be a note name such as C, Eb or G, got '{v}'"))
}

/// Semitones above C of a note name from `NOTES`.
pub fn semitones(name: &str) -> u8 {
    NOTES.iter().find(|(n, _)| *n == name).map_or(0, |(_, s)| *s)
}

/// Sets `slot` and reports whether anything changed.
pub fn set(slot: &mut &'static str, value: &'static str) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
}

/// What `apply` reports back: the new setting, or that it was already so.
pub fn outcome(changed: bool, description: String) -> String {
    if changed { description } else { format!("no change, already {description}") }
}
