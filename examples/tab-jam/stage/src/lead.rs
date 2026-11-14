//! Agent Lead: owns the melody's mood, how random it is, and which synth
//! voice plays it. Its first note of each bar lands on the bass root.

use std::cell::RefCell;

use serde_json::{json, Value};

use crate::music::{function, note, outcome, pick, rng, set, BLUES, PENTATONIC, STEPS_PER_BAR};

pub const SUGGESTIONS: [&str; 5] = [
    "Make the melody dark",
    "Lead goes frantic",
    "Lead goes jazz",
    "Set the lead randomness to 90",
    "Lead sound piano",
];

const MOODS: [&str; 6] = ["calm", "bright", "dark", "frantic", "jazz", "silent"];
const SOUNDS: [&str; 5] = ["saw", "supersaw", "piano", "square", "bell"];

struct Lead {
    mood: &'static str,
    /// 0 repeats one phrase every bar; 100 picks every note afresh.
    randomness: u32,
    /// Which voice the page's synthesizer uses.
    sound: &'static str,
}

thread_local! {
    static LEAD: RefCell<Lead> = const { RefCell::new(Lead { mood: "calm", randomness: 30, sound: "saw" }) };
}

/// The synth voice for the lead, sent with every rendered bar.
pub fn sound() -> &'static str {
    LEAD.with(|l| l.borrow().sound)
}

pub fn describe() -> Value {
    json!({ "agent": "Lead", "role": "Lead melody" })
}

pub fn brief() -> Value {
    json!({
        "system": "You are the lead melody player. Only act on requests about the melody or lead.",
        "aliases": ["lead", "melody", "solo", "synth", "randomness"],
        "tools": [
            function("set_lead_mood", "Change how the lead melody feels.",
                json!({ "mood": { "type": "string", "enum": MOODS } }), &["mood"]),
            function("set_lead_randomness", "Set how random or unpredictable the lead melody is, as a percent from 0 to 100.",
                json!({ "percent": { "type": "integer" } }), &["percent"]),
            // This wording is deliberate: with "sound" in the description or as the
            // argument name, the model stops answering mood cues such as "lead goes jazz".
            function("set_lead_sound", "Switch the lead to another instrument.",
                json!({ "instrument": { "type": "string", "enum": SOUNDS } }), &["instrument"]),
        ],
    })
}

pub fn pad(agent: usize) -> Value {
    json!({
        "agent": agent,
        "x": { "label": "mood", "tool": "set_lead_mood", "arg": "mood", "state": "lead", "values": MOODS },
        "y": { "label": "randomness", "tool": "set_lead_randomness", "arg": "percent", "state": "lead_randomness", "min": 0, "max": 100, "step": 5 },
        "choice": { "label": "sound", "tool": "set_lead_sound", "arg": "instrument", "state": "lead_sound", "values": SOUNDS },
    })
}

pub fn apply(name: &str, args: &Value) -> Result<String, String> {
    LEAD.with(|l| {
        let mut l = l.borrow_mut();
        match name {
            "set_lead_mood" => {
                let changed = set(&mut l.mood, pick(&MOODS, &args["mood"], "mood")?);
                Ok(outcome(changed, format!("lead {}", l.mood)))
            }
            "set_lead_randomness" => {
                let percent = args["percent"].as_u64().ok_or("`percent` must be a whole number")?;
                if percent > 100 {
                    return Err(format!("`percent` must be between 0 and 100, got {percent}"));
                }
                let changed = l.randomness != percent as u32;
                l.randomness = percent as u32;
                Ok(outcome(changed, format!("lead randomness {percent}%")))
            }
            "set_lead_sound" => {
                let changed = set(&mut l.sound, pick(&SOUNDS, &args["instrument"], "instrument")?);
                Ok(outcome(changed, format!("lead sound {}", l.sound)))
            }
            _ => Err(format!("the lead has no tool named '{name}'")),
        }
    })
}

pub fn state(state: &mut Value) {
    LEAD.with(|l| {
        let l = l.borrow();
        state["lead"] = json!(l.mood);
        state["lead_randomness"] = json!(l.randomness);
        state["lead_sound"] = json!(l.sound);
    })
}

/// The melody notes of one bar, over the band's `root`.
pub fn render(root: u8, bar: u32, out: &mut Vec<Value>) {
    let (mood, randomness) = LEAD.with(|l| {
        let l = l.borrow();
        (l.mood, l.randomness)
    });
    // (octave as midi C, allowed steps, chance to play out of 100, note length, scale)
    let every = |n: usize| (0..STEPS_PER_BAR).step_by(n).collect::<Vec<_>>();
    let (base, steps, density, len, scale): (u8, Vec<u32>, u32, u32, &[u8]) = match mood {
        "calm" => (60, every(4), 70, 4, &PENTATONIC),
        "bright" => (72, every(2), 65, 2, &PENTATONIC),
        "dark" => (48, every(8), 100, 8, &PENTATONIC),
        "frantic" => (72, every(1), 80, 1, &PENTATONIC),
        "jazz" => (60, vec![0, 3, 4, 7, 8, 11, 12, 15], 65, 2, &BLUES),
        _ => return,
    };
    // Two streams: `phrase` is the same every bar, `wild` changes with the bar.
    // Randomness is the chance that a decision comes from the wild stream.
    let mut phrase = rng(mood.len() as u32 * 7919 + 1);
    let mut wild = rng(bar * 31 + mood.len() as u32);
    let mut chooser = rng(bar * 131 + 7);
    let mut draw = |limit: u32| {
        let (p, w) = (phrase(), wild());
        (if chooser() % 100 < randomness { w } else { p }) % limit
    };
    for step in steps {
        let plays = draw(100) < density;
        let degree = draw(scale.len() as u32) as usize;
        let leap = draw(100);
        if step > 0 && !plays {
            continue;
        }
        // The first note of the bar lands on the bass root so the band locks together.
        let mut pitch = if step == 0 { root } else { scale[degree] };
        // A wild melody also jumps an octave now and then.
        if step > 0 && leap < randomness / 4 {
            pitch += 12;
        }
        out.push(note("lead", step, len, base + pitch, 85));
    }
}
