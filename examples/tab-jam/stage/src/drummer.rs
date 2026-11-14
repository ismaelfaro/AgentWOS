//! Agent Drummer: owns the drum pattern and the tempo the whole band follows.

use std::cell::RefCell;

use serde_json::{json, Value};

use crate::music::{function, note, outcome, pick, rng, set};

pub const SUGGESTIONS: [&str; 5] = [
    "Drums double time",
    "Make the drums half time",
    "Drums play a breakbeat",
    "Make the drums jazz",
    "Make the drums random",
];
/// Listed after the other agents' suggestions: a fast tempo makes the
/// director refuse a frantic lead, so trying these last keeps the list usable in order.
pub const TEMPO_SUGGESTIONS: [&str; 2] = ["Set the tempo to 140", "Set the tempo to 300"];

const STYLES: [&str; 7] = ["four_on_floor", "half_time", "double_time", "breakbeat", "jazz", "random", "silent"];

struct Drummer {
    tempo: u32,
    style: &'static str,
}

thread_local! {
    static DRUMMER: RefCell<Drummer> = const { RefCell::new(Drummer { tempo: 100, style: "four_on_floor" }) };
}

/// Beats per minute for the whole band.
pub fn tempo() -> u32 {
    DRUMMER.with(|d| d.borrow().tempo)
}

pub fn describe() -> Value {
    json!({ "agent": "Drummer", "role": "Drum pattern and tempo" })
}

pub fn brief() -> Value {
    json!({
        "system": "You are the drummer. Only act on requests about drums or tempo.",
        "aliases": ["drums", "drum", "drummer", "beat", "tempo", "bpm", "speed"],
        "tools": [
            function("set_drum_pattern", "Change the drum beat or drum style.",
                json!({ "style": { "type": "string", "enum": STYLES } }), &["style"]),
            function("set_tempo", "Set the tempo or speed of the song in beats per minute (bpm).",
                json!({ "bpm": { "type": "integer" } }), &["bpm"]),
        ],
    })
}

pub fn pad(agent: usize) -> Value {
    json!({
        "agent": agent,
        "x": { "label": "style", "tool": "set_drum_pattern", "arg": "style", "state": "drums", "values": STYLES },
        "y": { "label": "bpm", "tool": "set_tempo", "arg": "bpm", "state": "tempo", "min": 50, "max": 200, "step": 5 },
    })
}

pub fn apply(name: &str, args: &Value) -> Result<String, String> {
    DRUMMER.with(|d| {
        let mut d = d.borrow_mut();
        match name {
            "set_drum_pattern" => {
                let changed = set(&mut d.style, pick(&STYLES, &args["style"], "style")?);
                Ok(outcome(changed, format!("drums {}", d.style.replace('_', " "))))
            }
            "set_tempo" => {
                let bpm = args["bpm"].as_u64().ok_or("`bpm` must be a positive integer")?;
                if !(40..=240).contains(&bpm) {
                    return Err(format!("`bpm` must be between 40 and 240, got {bpm}"));
                }
                let changed = d.tempo != bpm as u32;
                d.tempo = bpm as u32;
                Ok(outcome(changed, format!("tempo {bpm} bpm")))
            }
            _ => Err(format!("the drummer has no tool named '{name}'")),
        }
    })
}

pub fn state(state: &mut Value) {
    DRUMMER.with(|d| {
        let d = d.borrow();
        state["tempo"] = json!(d.tempo);
        state["drums"] = json!(d.style);
    })
}

/// Kick, snare and hat hits of one bar.
pub fn render(bar: u32, out: &mut Vec<Value>) {
    let style = DRUMMER.with(|d| d.borrow().style);
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
