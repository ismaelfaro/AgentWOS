//! Agent Bassist: owns the bass line and the root note everyone else tunes to.

use std::cell::RefCell;

use serde_json::{json, Value};

use crate::music::{function, note, note_name, outcome, pick, semitones, set};

pub const SUGGESTIONS: [&str; 4] = ["Bass walking on F", "Bass octaves on G", "Funky bass on D", "Bass on A"];

const MOVES: [&str; 5] = ["steady", "walking", "octaves", "funky", "silent"];

struct Bassist {
    root: &'static str,
    movement: &'static str,
}

thread_local! {
    static BASSIST: RefCell<Bassist> = const { RefCell::new(Bassist { root: "C", movement: "steady" }) };
}

/// The band's root, as semitones above C. Lead and orchestra follow it.
pub fn root() -> u8 {
    BASSIST.with(|b| semitones(b.borrow().root))
}

pub fn describe() -> Value {
    json!({ "agent": "Bassist", "role": "Bass line" })
}

pub fn brief() -> Value {
    json!({
        "system": "You are the bass player. Only act on requests about the bass.",
        "aliases": ["bass", "bassist", "bassline"],
        "tools": [
            function("set_bass_line", "Change the bass line: its root note and how it moves.", json!({
                "root": { "type": "string", "description": "Root note letter, e.g. C, D, Eb, F, G, A, Bb" },
                "movement": { "type": "string", "enum": MOVES },
            }), &["root"]),
        ],
    })
}

pub fn pad(agent: usize) -> Value {
    json!({
        "agent": agent,
        "x": { "label": "root", "tool": "set_bass_line", "arg": "root", "state": "bass_root", "values": ["C", "D", "Eb", "F", "G", "Ab", "Bb"] },
        "y": { "label": "movement", "tool": "set_bass_line", "arg": "movement", "state": "bass_movement", "values": MOVES },
    })
}

pub fn apply(name: &str, args: &Value) -> Result<String, String> {
    if name != "set_bass_line" {
        return Err(format!("the bassist has no tool named '{name}'"));
    }
    BASSIST.with(|b| {
        let mut b = b.borrow_mut();
        // Either argument may be missing: the bass keeps what it had.
        let root = if args["root"].is_null() { b.root } else { note_name(&args["root"])? };
        let movement = if args["movement"].is_null() { b.movement } else { pick(&MOVES, &args["movement"], "movement")? };
        let changed = set(&mut b.root, root) | set(&mut b.movement, movement);
        Ok(outcome(changed, format!("bass {movement} on {root}")))
    })
}

pub fn state(state: &mut Value) {
    BASSIST.with(|b| {
        let b = b.borrow();
        state["bass_root"] = json!(b.root);
        state["bass_movement"] = json!(b.movement);
    })
}

/// The bass notes of one bar.
pub fn render(out: &mut Vec<Value>) {
    let movement = BASSIST.with(|b| b.borrow().movement);
    let base = 36 + root();
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
