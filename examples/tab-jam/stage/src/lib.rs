wit_bindgen::generate!({
    world: "stage-module",
    path: "../../../wit/world.wit",
});

use std::cell::RefCell;

use exports::agentwos::core::stage::Guest;
use serde_json::{json, Value};

struct Component;

const DRUM_STYLES: [&str; 4] = ["four_on_floor", "half_time", "breakbeat", "silent"];
const BASS_ROOTS: [(&str, u8); 6] = [("C", 0), ("Eb", 3), ("F", 5), ("G", 7), ("Ab", 8), ("Bb", 10)];
const BASS_MOVES: [&str; 4] = ["steady", "walking", "octaves", "silent"];
const LEAD_MOODS: [&str; 5] = ["calm", "bright", "dark", "frantic", "silent"];
/// C minor pentatonic, as semitones above C.
const SCALE: [u8; 5] = [0, 3, 5, 7, 10];
const STEPS_PER_BAR: u32 = 16;

struct Band {
    tempo: u32,
    drums: &'static str,
    bass_root: &'static str,
    bass_move: &'static str,
    lead: &'static str,
}

thread_local! {
    static BAND: RefCell<Band> = const { RefCell::new(Band {
        tempo: 100,
        drums: "four_on_floor",
        bass_root: "C",
        bass_move: "steady",
        lead: "calm",
    }) };
}

fn pick(options: &[&'static str], value: &Value, field: &str) -> Result<&'static str, String> {
    let v = value.as_str().ok_or_else(|| format!("`{field}` must be a string"))?;
    options
        .iter()
        .copied()
        .find(|o| o.eq_ignore_ascii_case(v))
        .ok_or_else(|| format!("`{field}` must be one of {options:?}, got '{v}'"))
}

fn function(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({ "type": "function", "function": {
        "name": name, "description": description,
        "parameters": { "type": "object", "properties": properties, "required": required },
    }})
}

fn note(inst: &str, step: u32, len: u32, midi: u8, vel: u8) -> Value {
    json!({ "inst": inst, "step": step, "len": len, "midi": midi, "vel": vel })
}

/// Small deterministic generator so a bar always renders the same way.
fn rng(seed: u32) -> impl FnMut() -> u32 {
    let mut x = seed.wrapping_mul(2654435761).max(1);
    move || {
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        x
    }
}

fn drums(style: &str, out: &mut Vec<Value>) {
    let (kicks, snares, hat_every): (&[u32], &[u32], u32) = match style {
        "four_on_floor" => (&[0, 4, 8, 12], &[4, 12], 2),
        "half_time" => (&[0, 10], &[8], 4),
        "breakbeat" => (&[0, 6, 10], &[4, 12, 15], 2),
        _ => return,
    };
    for &s in kicks {
        out.push(note("kick", s, 1, 36, 120));
    }
    for &s in snares {
        out.push(note("snare", s, 1, 38, if s == 15 { 60 } else { 105 }));
    }
    for s in (0..STEPS_PER_BAR).step_by(hat_every as usize) {
        out.push(note("hat", s, 1, 42, if s % 4 == 0 { 70 } else { 45 }));
    }
}

fn bass(root: u8, movement: &str, out: &mut Vec<Value>) {
    let base = 36 + root;
    match movement {
        "steady" => (0..8).for_each(|i| out.push(note("bass", i * 2, 2, base, if i % 2 == 0 { 105 } else { 80 }))),
        "walking" => [0u8, 3, 7, 10]
            .iter()
            .enumerate()
            .for_each(|(i, off)| out.push(note("bass", i as u32 * 4, 4, base + off, 100))),
        "octaves" => (0..8).for_each(|i| out.push(note("bass", i * 2, 2, base + if i % 2 == 0 { 0 } else { 12 }, 95))),
        _ => {}
    }
}

fn lead(mood: &str, root: u8, bar: u32, out: &mut Vec<Value>) {
    // (base octave as midi C, step size in 16ths, chance to play out of 100, note length)
    let (base, grid, density, len): (u8, u32, u32, u32) = match mood {
        "calm" => (60, 4, 70, 4),
        "bright" => (72, 2, 65, 2),
        "dark" => (48, 8, 100, 8),
        "frantic" => (72, 1, 80, 1),
        _ => return,
    };
    let mut next = rng(bar * 31 + mood.len() as u32);
    for step in (0..STEPS_PER_BAR).step_by(grid as usize) {
        if step > 0 && next() % 100 >= density {
            continue;
        }
        // The first note of the bar lands on the bass root so the band locks together.
        let pitch = if step == 0 { root } else { SCALE[(next() % SCALE.len() as u32) as usize] };
        out.push(note("lead", step, len, base + pitch, 85));
    }
}

impl Guest for Component {
    fn setup(_config: String) -> String {
        json!({
            "agents": [
                { "agent": "Drummer", "role": "Drum pattern and tempo" },
                { "agent": "Bassist", "role": "Bass line" },
                { "agent": "Lead", "role": "Lead melody" },
            ],
            "key": "C minor",
            "steps_per_bar": STEPS_PER_BAR,
        })
        .to_string()
    }

    fn brief(agent: u32) -> String {
        match agent {
            0 => json!({
                "system": "You are the drummer. Only act on requests about drums or tempo.",
                "aliases": ["drums", "drum", "drummer", "beat", "tempo", "bpm", "speed"],
                "tools": [
                    function("set_drum_pattern", "Change the drum beat.",
                        json!({ "style": { "type": "string", "enum": DRUM_STYLES } }), &["style"]),
                    function("set_tempo", "Set the song speed in beats per minute.",
                        json!({ "bpm": { "type": "integer" } }), &["bpm"]),
                ],
            }),
            1 => json!({
                "system": "You are the bass player. Only act on requests about the bass.",
                "aliases": ["bass", "bassist", "bassline"],
                "tools": [
                    function("set_bass_line", "Change the bass line.", json!({
                        "root": { "type": "string", "enum": BASS_ROOTS.map(|(n, _)| n) },
                        "movement": { "type": "string", "enum": BASS_MOVES },
                    }), &["root", "movement"]),
                ],
            }),
            _ => json!({
                "system": "You are the lead melody player. Only act on requests about the melody or lead.",
                "aliases": ["lead", "melody", "solo", "synth"],
                "tools": [
                    function("set_lead_mood", "Change how the lead melody feels.",
                        json!({ "mood": { "type": "string", "enum": LEAD_MOODS } }), &["mood"]),
                ],
            }),
        }
        .to_string()
    }

    fn context() -> String {
        Self::state()
    }

    fn apply(agent: u32, call: String) -> Result<String, String> {
        let call: Value = serde_json::from_str(&call).map_err(|e| format!("call is not JSON: {e}"))?;
        let name = call["name"].as_str().unwrap_or("");
        let args = &call["arguments"];
        BAND.with(|b| {
            let mut b = b.borrow_mut();
            match (agent, name) {
                (0, "set_drum_pattern") => {
                    b.drums = pick(&DRUM_STYLES, &args["style"], "style")?;
                    Ok(format!("drums: {}", b.drums))
                }
                (0, "set_tempo") => {
                    let bpm = args["bpm"].as_u64().ok_or("`bpm` must be a positive integer")?;
                    if !(40..=240).contains(&bpm) {
                        return Err(format!("`bpm` must be between 40 and 240, got {bpm}"));
                    }
                    b.tempo = bpm as u32;
                    Ok(format!("tempo: {bpm} bpm"))
                }
                (1, "set_bass_line") => {
                    let root = pick(&BASS_ROOTS.map(|(n, _)| n), &args["root"], "root")?;
                    let movement = pick(&BASS_MOVES, &args["movement"], "movement")?;
                    b.bass_root = root;
                    b.bass_move = movement;
                    Ok(format!("bass: {movement} on {root}"))
                }
                (2, "set_lead_mood") => {
                    b.lead = pick(&LEAD_MOODS, &args["mood"], "mood")?;
                    Ok(format!("lead: {}", b.lead))
                }
                _ => Err(format!("agent {agent} has no tool named '{name}'")),
            }
        })
    }

    fn render(tick: u32) -> String {
        BAND.with(|b| {
            let b = b.borrow();
            let root = BASS_ROOTS.iter().find(|(n, _)| *n == b.bass_root).map(|(_, s)| *s).unwrap_or(0);
            let mut notes = Vec::new();
            drums(b.drums, &mut notes);
            bass(root, b.bass_move, &mut notes);
            lead(b.lead, root, tick, &mut notes);
            json!({ "bar": tick, "tempo": b.tempo, "notes": notes }).to_string()
        })
    }

    fn state() -> String {
        BAND.with(|b| {
            let b = b.borrow();
            json!({
                "tempo": b.tempo,
                "drums": b.drums,
                "bass_root": b.bass_root,
                "bass_movement": b.bass_move,
                "lead": b.lead,
            })
            .to_string()
        })
    }
}

export!(Component);
