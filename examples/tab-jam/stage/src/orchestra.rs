//! Orchestra sections: agents added one by one while the band plays. Each
//! holds one note of the chord and differs from the others in timbre, register,
//! interval, detune and pulse. All of them tune to the bass root and pulse on a
//! division of the bar, so the orchestra stays together.

use std::cell::RefCell;

use serde_json::{json, Value};

use crate::music::{function, outcome, pick, set, STEPS_PER_BAR};

/// Sections are handed out in this order as agents are added.
const SECTIONS: [&str; 5] = ["Strings", "Brass", "Flute", "Choir", "Organ"];
/// Chord notes a section can hold, as (name, semitones above the root).
const INTERVALS: [(&str, u8); 6] = [("root", 0), ("third", 3), ("fifth", 7), ("seventh", 10), ("octave", 12), ("silent", 0)];
/// Pulse lengths as (name, sixteenth notes per pulse).
const PULSES: [(&str, u32); 5] = [("bar", 16), ("half", 8), ("quarter", 4), ("eighth", 2), ("sixteenth", 1)];
const MAX_SECTIONS: usize = 48;

struct Section {
    /// Fixed when the agent joins. Also the synth timbre.
    kind: &'static str,
    interval: &'static str,
    detune: u32,
    pulse: &'static str,
}

impl Section {
    /// Starting settings that differ from one section to the next.
    fn numbered(i: usize) -> Self {
        Section {
            kind: SECTIONS[i % SECTIONS.len()],
            interval: ["root", "fifth", "octave", "third", "seventh"][i % 5],
            detune: (4 + i as u32 * 7) % 26,
            pulse: ["quarter", "eighth", "half", "sixteenth", "bar"][i % 5],
        }
    }
}

thread_local! {
    static SECTIONS_ON_STAGE: RefCell<Vec<Section>> = const { RefCell::new(Vec::new()) };
}

fn names(options: &[(&'static str, impl Copy)]) -> Vec<&'static str> {
    options.iter().map(|(n, _)| *n).collect()
}

/// "Strings" for the first of a kind, "Strings 2" for the next one.
fn title(i: usize) -> String {
    let round = i / SECTIONS.len();
    let kind = SECTIONS[i % SECTIONS.len()];
    if round == 0 { kind.to_string() } else { format!("{kind} {}", round + 1) }
}

/// The word that addresses this agent in a cue: "strings", "strings2".
fn alias(i: usize) -> String {
    title(i).to_lowercase().replace(' ', "")
}

/// How many sections have joined.
pub fn count() -> usize {
    SECTIONS_ON_STAGE.with(|s| s.borrow().len())
}

/// Adds the next section. `agent` is the agent index it will answer to.
/// Returns what the kernel and the page need to know about it.
pub fn join(agent: usize) -> Result<Value, String> {
    SECTIONS_ON_STAGE.with(|s| {
        let mut s = s.borrow_mut();
        if s.len() >= MAX_SECTIONS {
            return Err(format!("the stage holds at most {MAX_SECTIONS} orchestra sections"));
        }
        let i = s.len();
        s.push(Section::numbered(i));
        Ok(json!({
            "index": agent,
            "agent": title(i),
            "role": "Orchestra section: holds one note of the chord, in time with the band",
            "pad": pad(i, agent),
        }))
    })
}

pub fn brief(i: usize) -> Value {
    let (title, alias) = (title(i), alias(i));
    json!({
        "system": format!("You are the {title} section of an orchestra. Only act on requests for {alias}."),
        "aliases": [alias],
        "tools": [
            function("set_drone_interval", "Choose which note of the chord this section holds.",
                json!({ "interval": { "type": "string", "enum": names(&INTERVALS) } }), &["interval"]),
            function("set_drone_detune", "Set how many cents this section is detuned.",
                json!({ "cents": { "type": "integer" } }), &["cents"]),
            function("set_drone_pulse", "Set how long each pulse of this section lasts.",
                json!({ "pulse": { "type": "string", "enum": names(&PULSES) } }), &["pulse"]),
        ],
    })
}

fn pad(i: usize, agent: usize) -> Value {
    let n = i + 1;
    json!({
        "agent": agent,
        "x": { "label": "detune", "tool": "set_drone_detune", "arg": "cents", "state": format!("drone{n}_detune"), "min": 0, "max": 50, "step": 2 },
        "y": { "label": "pulse", "tool": "set_drone_pulse", "arg": "pulse", "state": format!("drone{n}_pulse"), "values": names(&PULSES) },
        "choice": { "label": "holds", "tool": "set_drone_interval", "arg": "interval", "state": format!("drone{n}_interval"), "values": names(&INTERVALS) },
    })
}

pub fn apply(i: usize, name: &str, args: &Value) -> Result<String, String> {
    SECTIONS_ON_STAGE.with(|s| {
        let mut s = s.borrow_mut();
        let n = title(i);
        let section = s.get_mut(i).ok_or_else(|| format!("there is no agent {n}"))?;
        match name {
            "set_drone_interval" => {
                let changed = set(&mut section.interval, pick(&names(&INTERVALS), &args["interval"], "interval")?);
                Ok(outcome(changed, format!("{n} holds the {}", section.interval)))
            }
            "set_drone_detune" => {
                let cents = args["cents"].as_u64().ok_or("`cents` must be a whole number")?;
                if cents > 100 {
                    return Err(format!("`cents` must be between 0 and 100, got {cents}"));
                }
                let changed = section.detune != cents as u32;
                section.detune = cents as u32;
                Ok(outcome(changed, format!("{n} detuned {cents} cents")))
            }
            "set_drone_pulse" => {
                let changed = set(&mut section.pulse, pick(&names(&PULSES), &args["pulse"], "pulse")?);
                Ok(outcome(changed, format!("{n} pulses every {}", section.pulse)))
            }
            _ => Err(format!("an orchestra section has no tool named '{name}'")),
        }
    })
}

pub fn state(state: &mut Value) {
    SECTIONS_ON_STAGE.with(|s| {
        let s = s.borrow();
        state["drone_count"] = json!(s.len());
        for (i, section) in s.iter().enumerate() {
            let n = i + 1;
            state[format!("drone{n}_interval")] = json!(section.interval);
            state[format!("drone{n}_detune")] = json!(section.detune);
            state[format!("drone{n}_pulse")] = json!(section.pulse);
        }
    })
}

/// One held note per sounding section for the bar, tuned to the band's `root`.
pub fn render(root: u8, out: &mut Vec<Value>) {
    SECTIONS_ON_STAGE.with(|s| {
        let s = s.borrow();
        let sounding = s.iter().filter(|x| x.interval != "silent").count();
        for (i, section) in s.iter().enumerate() {
            if section.interval == "silent" {
                continue;
            }
            let offset = INTERVALS.iter().find(|(n, _)| *n == section.interval).map_or(0, |(_, o)| *o);
            let pulse = PULSES.iter().find(|(n, _)| *n == section.pulse).map_or(4, |(_, p)| *p);
            // Spread the orchestra over three octaves so sections do not pile on one pitch.
            let register = [48u8, 60, 36][i % 3];
            out.push(json!({
                "inst": "drone", "timbre": section.kind.to_lowercase(), "voice": i, "count": sounding,
                "step": 0, "len": STEPS_PER_BAR,
                "midi": register + root + offset, "vel": 80, "detune": section.detune, "pulse": pulse,
            }));
        }
    })
}
