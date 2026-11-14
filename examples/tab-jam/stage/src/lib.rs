wit_bindgen::generate!({
    world: "stage-module",
    path: "../../../wit/world.wit",
});

use std::cell::RefCell;

use exports::agentwos::core::stage::Guest;
use serde_json::{json, Value};

struct Component;

const DRUM_STYLES: [&str; 7] = ["four_on_floor", "half_time", "double_time", "breakbeat", "jazz", "random", "silent"];
const BASS_MOVES: [&str; 5] = ["steady", "walking", "octaves", "funky", "silent"];
const LEAD_MOODS: [&str; 6] = ["calm", "bright", "dark", "frantic", "jazzy", "silent"];
/// Every note name the bass accepts, as semitones above C. The sound engineer
/// control decides whether a root fits the key; the stage only needs a pitch.
const NOTES: [(&str, u8); 17] = [
    ("C", 0), ("C#", 1), ("Db", 1), ("D", 2), ("D#", 3), ("Eb", 3), ("E", 4), ("F", 5), ("F#", 6),
    ("Gb", 6), ("G", 7), ("G#", 8), ("Ab", 8), ("A", 9), ("A#", 10), ("Bb", 10), ("B", 11),
];
/// C minor pentatonic and C blues, as semitones above C.
const PENTATONIC: [u8; 5] = [0, 3, 5, 7, 10];
const BLUES: [u8; 6] = [0, 3, 5, 6, 7, 10];
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
    let wanted = v.trim().to_lowercase().replace(' ', "_");
    options
        .iter()
        .copied()
        .find(|o| *o == wanted)
        .ok_or_else(|| format!("`{field}` must be one of {}, got '{v}'", options.join(", ")))
}

fn note_name(value: &Value) -> Result<&'static str, String> {
    let v = value.as_str().ok_or("`root` must be a note name")?;
    NOTES
        .iter()
        .map(|(n, _)| *n)
        .find(|n| n.eq_ignore_ascii_case(v.trim()))
        .ok_or_else(|| format!("`root` must be a note name such as C, Eb or G, got '{v}'"))
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

fn drums(style: &str, bar: u32, out: &mut Vec<Value>) {
    let mut hit = |inst: &str, step: u32, vel: u8| {
        let midi = match inst {
            "kick" => 36,
            "snare" => 38,
            _ => 42,
        };
        out.push(note(inst, step, 1, midi, vel));
    };
    match style {
        "four_on_floor" => {
            [0, 4, 8, 12].iter().for_each(|&s| hit("kick", s, 120));
            [4, 12].iter().for_each(|&s| hit("snare", s, 105));
            (0..16).step_by(2).for_each(|s| hit("hat", s, if s % 4 == 0 { 70 } else { 45 }));
        }
        "half_time" => {
            [0, 10].iter().for_each(|&s| hit("kick", s, 120));
            hit("snare", 8, 110);
            (0..16).step_by(4).for_each(|s| hit("hat", s, 60));
        }
        "double_time" => {
            (0..16).step_by(2).for_each(|s| hit("kick", s, if s % 4 == 0 { 120 } else { 90 }));
            [2, 6, 10, 14].iter().for_each(|&s| hit("snare", s, 105));
            (0..16).for_each(|s| hit("hat", s, if s % 2 == 0 { 65 } else { 40 }));
        }
        "breakbeat" => {
            [0, 6, 10].iter().for_each(|&s| hit("kick", s, 120));
            [4, 12].iter().for_each(|&s| hit("snare", s, 105));
            hit("snare", 15, 60);
            (0..16).step_by(2).for_each(|s| hit("hat", s, if s % 4 == 0 { 70 } else { 45 }));
        }
        "jazz" => {
            // Swing ride: beat, then the late off-beat.
            [0, 4, 7, 8, 12, 15].iter().for_each(|&s| hit("hat", s, if s % 4 == 0 { 75 } else { 50 }));
            [0, 10].iter().for_each(|&s| hit("kick", s, 80));
            [7, 13].iter().for_each(|&s| hit("snare", s, 45));
        }
        "random" => {
            let mut next = rng(bar * 97 + 13);
            hit("kick", 0, 115);
            for s in 1..16 {
                match next() % 10 {
                    0 | 1 => hit("kick", s, 100),
                    2 => hit("snare", s, 95),
                    3..=5 => hit("hat", s, 55),
                    _ => {}
                }
            }
        }
        _ => {}
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
        "funky" => [(0u32, 0u8, 110u8), (3, 0, 85), (6, 12, 100), (8, 0, 105), (11, 10, 85), (14, 12, 100)]
            .iter()
            .for_each(|&(step, off, vel)| out.push(note("bass", step, 1, base + off, vel))),
        _ => {}
    }
}

fn lead(mood: &str, root: u8, bar: u32, out: &mut Vec<Value>) {
    // (octave as midi C, allowed steps, chance to play out of 100, note length, scale)
    let every = |n: usize| (0..STEPS_PER_BAR).step_by(n).collect::<Vec<_>>();
    let (base, steps, density, len, scale): (u8, Vec<u32>, u32, u32, &[u8]) = match mood {
        "calm" => (60, every(4), 70, 4, &PENTATONIC),
        "bright" => (72, every(2), 65, 2, &PENTATONIC),
        "dark" => (48, every(8), 100, 8, &PENTATONIC),
        "frantic" => (72, every(1), 80, 1, &PENTATONIC),
        "jazzy" => (60, vec![0, 3, 4, 7, 8, 11, 12, 15], 65, 2, &BLUES),
        _ => return,
    };
    let mut next = rng(bar * 31 + mood.len() as u32);
    for step in steps {
        if step > 0 && next() % 100 >= density {
            continue;
        }
        // The first note of the bar lands on the bass root so the band locks together.
        let pitch = if step == 0 { root } else { scale[(next() % scale.len() as u32) as usize] };
        out.push(note("lead", step, len, base + pitch, 85));
    }
}

/// Sets `slot` and reports whether anything changed.
fn set(slot: &mut &'static str, value: &'static str) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
}

fn outcome(changed: bool, description: String) -> String {
    if changed { description } else { format!("no change, already {description}") }
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
            "suggestions": [
                "Drums double time",
                "Make the drums half time",
                "Drums play a breakbeat",
                "Make the drums jazz",
                "Make the drums random",
                "Bass walking on F",
                "Bass octaves on G",
                "Funky bass on D",
                "Bass on A",
                "Make the melody dark",
                "Lead goes frantic",
                "Lead goes jazz",
                "Set the tempo to 140",
                "Set the tempo to 300",
                "Silence the drums and make the melody bright",
            ],
        })
        .to_string()
    }

    fn brief(agent: u32) -> String {
        match agent {
            0 => json!({
                "system": "You are the drummer. Only act on requests about drums or tempo.",
                "aliases": ["drums", "drum", "drummer", "beat", "tempo", "bpm", "speed"],
                "tools": [
                    function("set_drum_pattern", "Change the drum beat or drum style.",
                        json!({ "style": { "type": "string", "enum": DRUM_STYLES } }), &["style"]),
                    function("set_tempo", "Set the tempo or speed of the song in beats per minute (bpm).",
                        json!({ "bpm": { "type": "integer" } }), &["bpm"]),
                ],
            }),
            1 => json!({
                "system": "You are the bass player. Only act on requests about the bass.",
                "aliases": ["bass", "bassist", "bassline"],
                "tools": [
                    function("set_bass_line", "Change the bass line: its root note and how it moves.", json!({
                        "root": { "type": "string", "description": "Root note letter, e.g. C, D, Eb, F, G, A, Bb" },
                        "movement": { "type": "string", "enum": BASS_MOVES },
                    }), &["root"]),
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
                    let changed = set(&mut b.drums, pick(&DRUM_STYLES, &args["style"], "style")?);
                    Ok(outcome(changed, format!("drums {}", b.drums.replace('_', " "))))
                }
                (0, "set_tempo") => {
                    let bpm = args["bpm"].as_u64().ok_or("`bpm` must be a positive integer")?;
                    if !(40..=240).contains(&bpm) {
                        return Err(format!("`bpm` must be between 40 and 240, got {bpm}"));
                    }
                    let changed = b.tempo != bpm as u32;
                    b.tempo = bpm as u32;
                    Ok(outcome(changed, format!("tempo {bpm} bpm")))
                }
                (1, "set_bass_line") => {
                    // Either argument may be missing: the bass keeps what it had.
                    let root = if args["root"].is_null() { b.bass_root } else { note_name(&args["root"])? };
                    let movement = if args["movement"].is_null() {
                        b.bass_move
                    } else {
                        pick(&BASS_MOVES, &args["movement"], "movement")?
                    };
                    let changed = set(&mut b.bass_root, root) | set(&mut b.bass_move, movement);
                    Ok(outcome(changed, format!("bass {movement} on {root}")))
                }
                (2, "set_lead_mood") => {
                    let changed = set(&mut b.lead, pick(&LEAD_MOODS, &args["mood"], "mood")?);
                    Ok(outcome(changed, format!("lead {}", b.lead)))
                }
                _ => Err(format!("agent {agent} has no tool named '{name}'")),
            }
        })
    }

    fn render(tick: u32) -> String {
        BAND.with(|b| {
            let b = b.borrow();
            let root = NOTES.iter().find(|(n, _)| *n == b.bass_root).map(|(_, s)| *s).unwrap_or(0);
            let mut notes = Vec::new();
            drums(b.drums, tick, &mut notes);
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
                "key": "C minor",
            })
            .to_string()
        })
    }
}

export!(Component);
