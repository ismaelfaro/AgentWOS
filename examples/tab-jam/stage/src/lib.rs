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
const LEAD_MOODS: [&str; 6] = ["calm", "bright", "dark", "frantic", "jazz", "silent"];
const LEAD_SOUNDS: [&str; 5] = ["saw", "supersaw", "piano", "square", "bell"];
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

/// Drones: extra players that hold one note of the chord. Each one differs in
/// interval, detune, pulse and register, and all follow the bass root and the bar.
const DRONE_INTERVALS: [(&str, u8); 6] = [("root", 0), ("third", 3), ("fifth", 7), ("seventh", 10), ("octave", 12), ("silent", 0)];
/// Pulse lengths as (name, sixteenth notes per pulse).
const DRONE_PULSES: [(&str, u32); 5] = [("bar", 16), ("half", 8), ("quarter", 4), ("eighth", 2), ("sixteenth", 1)];
/// Orchestra sections, handed out in turn as agents are added.
const SECTIONS: [&str; 5] = ["Strings", "Brass", "Flute", "Choir", "Organ"];
const FIRST_DRONE_AGENT: u32 = 3;
const MAX_DRONES: usize = 16;

struct Drone {
    /// Which section it is; fixed when the agent joins. Also the synth timbre.
    section: &'static str,
    interval: &'static str,
    detune: u32,
    pulse: &'static str,
}

impl Drone {
    /// Starting settings that differ from one drone to the next.
    fn numbered(i: usize) -> Self {
        Drone {
            section: SECTIONS[i % SECTIONS.len()],
            interval: ["root", "fifth", "octave", "third", "seventh"][i % 5],
            detune: (4 + i as u32 * 7) % 26,
            pulse: ["quarter", "eighth", "half", "sixteenth", "bar"][i % 5],
        }
    }
}

/// "Strings" for the first of a section, "Strings 2" for the next one.
fn drone_title(i: usize) -> String {
    let round = i / SECTIONS.len();
    let section = SECTIONS[i % SECTIONS.len()];
    if round == 0 { section.to_string() } else { format!("{section} {}", round + 1) }
}

/// The word that addresses this agent in a cue: "strings", "strings2".
fn drone_alias(i: usize) -> String {
    drone_title(i).to_lowercase().replace(' ', "")
}

fn drone_names(options: &[(&'static str, impl Copy)]) -> Vec<&'static str> {
    options.iter().map(|(n, _)| *n).collect()
}

struct Band {
    tempo: u32,
    drums: &'static str,
    bass_root: &'static str,
    bass_move: &'static str,
    lead: &'static str,
    /// 0 repeats one phrase every bar; 100 picks every note afresh.
    lead_randomness: u32,
    /// Which voice the page's synthesizer uses for the lead.
    lead_sound: &'static str,
}

thread_local! {
    static DRONES: RefCell<Vec<Drone>> = const { RefCell::new(Vec::new()) };
    static BAND: RefCell<Band> = const { RefCell::new(Band {
        tempo: 100,
        drums: "four_on_floor",
        bass_root: "C",
        bass_move: "steady",
        lead: "calm",
        lead_randomness: 30,
        lead_sound: "saw",
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

fn lead(mood: &str, randomness: u32, root: u8, bar: u32, out: &mut Vec<Value>) {
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

/// Sets `slot` and reports whether anything changed.
fn set(slot: &mut &'static str, value: &'static str) -> bool {
    let changed = *slot != value;
    *slot = value;
    changed
}

fn outcome(changed: bool, description: String) -> String {
    if changed { description } else { format!("no change, already {description}") }
}

fn drone_pad(i: usize) -> Value {
    let n = i + 1;
    json!({
        "agent": FIRST_DRONE_AGENT as usize + i,
        "x": { "label": "detune", "tool": "set_drone_detune", "arg": "cents", "state": format!("drone{n}_detune"), "min": 0, "max": 50, "step": 2 },
        "y": { "label": "pulse", "tool": "set_drone_pulse", "arg": "pulse", "state": format!("drone{n}_pulse"), "values": drone_names(&DRONE_PULSES) },
        "choice": { "label": "holds", "tool": "set_drone_interval", "arg": "interval", "state": format!("drone{n}_interval"), "values": drone_names(&DRONE_INTERVALS) },
    })
}

fn drone_brief(i: usize) -> Value {
    let (title, alias) = (drone_title(i), drone_alias(i));
    json!({
        "system": format!("You are the {title} section of an orchestra. Only act on requests for {alias}."),
        "aliases": [alias],
        "tools": [
            function("set_drone_interval", "Choose which note of the chord this section holds.",
                json!({ "interval": { "type": "string", "enum": drone_names(&DRONE_INTERVALS) } }), &["interval"]),
            function("set_drone_detune", "Set how many cents this section is detuned.",
                json!({ "cents": { "type": "integer" } }), &["cents"]),
            function("set_drone_pulse", "Set how long each pulse of this section lasts.",
                json!({ "pulse": { "type": "string", "enum": drone_names(&DRONE_PULSES) } }), &["pulse"]),
        ],
    })
}

fn apply_to_drone(i: usize, name: &str, args: &Value) -> Result<String, String> {
    DRONES.with(|d| {
        let mut d = d.borrow_mut();
        let n = drone_title(i);
        let drone = d.get_mut(i).ok_or_else(|| format!("there is no agent {n}"))?;
        match name {
            "set_drone_interval" => {
                let changed = set(&mut drone.interval, pick(&drone_names(&DRONE_INTERVALS), &args["interval"], "interval")?);
                Ok(outcome(changed, format!("{n} holds the {}", drone.interval)))
            }
            "set_drone_detune" => {
                let cents = args["cents"].as_u64().ok_or("`cents` must be a whole number")?;
                if cents > 100 {
                    return Err(format!("`cents` must be between 0 and 100, got {cents}"));
                }
                let changed = drone.detune != cents as u32;
                drone.detune = cents as u32;
                Ok(outcome(changed, format!("{n} detuned {cents} cents")))
            }
            "set_drone_pulse" => {
                let changed = set(&mut drone.pulse, pick(&drone_names(&DRONE_PULSES), &args["pulse"], "pulse")?);
                Ok(outcome(changed, format!("{n} pulses every {}", drone.pulse)))
            }
            _ => Err(format!("a drone has no tool named '{name}'")),
        }
    })
}

/// One held note per sounding drone for the bar, tuned to the bass root.
fn drones(root: u8, out: &mut Vec<Value>) {
    DRONES.with(|d| {
        let d = d.borrow();
        let sounding = d.iter().filter(|x| x.interval != "silent").count();
        for (i, drone) in d.iter().enumerate() {
            if drone.interval == "silent" {
                continue;
            }
            let offset = DRONE_INTERVALS.iter().find(|(n, _)| *n == drone.interval).map_or(0, |(_, s)| *s);
            let pulse = DRONE_PULSES.iter().find(|(n, _)| *n == drone.pulse).map_or(4, |(_, s)| *s);
            // Spread the section over three octaves so drones do not pile on one pitch.
            let register = [48u8, 60, 36][i % 3];
            out.push(json!({
                "inst": "drone", "timbre": drone.section.to_lowercase(), "voice": i, "count": sounding,
                "step": 0, "len": STEPS_PER_BAR,
                "midi": register + root + offset, "vel": 80, "detune": drone.detune, "pulse": pulse,
            }));
        }
    })
}

impl Guest for Component {
    fn spawn(kind: String) -> Result<String, String> {
        if kind != "drone" {
            return Err(format!("this stage can only add an orchestra 'drone', not '{kind}'"));
        }
        DRONES.with(|d| {
            let mut d = d.borrow_mut();
            if d.len() >= MAX_DRONES {
                return Err(format!("the stage holds at most {MAX_DRONES} drones"));
            }
            let i = d.len();
            d.push(Drone::numbered(i));
            Ok(json!({
                "index": FIRST_DRONE_AGENT as usize + i,
                "agent": drone_title(i),
                "role": "Orchestra section: holds one note of the chord, in time with the band",
                "pad": drone_pad(i),
            })
            .to_string())
        })
    }

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
                "Set the lead randomness to 90",
                "Lead sound piano",
            ],
            // Touch pads for the UI: two settings per agent, each mapped to one
            // tool argument and to the state field that shows its current value.
            "pads": [
                { "agent": 0,
                  "x": { "label": "style", "tool": "set_drum_pattern", "arg": "style", "state": "drums", "values": DRUM_STYLES },
                  "y": { "label": "bpm", "tool": "set_tempo", "arg": "bpm", "state": "tempo", "min": 50, "max": 200, "step": 5 } },
                { "agent": 1,
                  "x": { "label": "root", "tool": "set_bass_line", "arg": "root", "state": "bass_root", "values": ["C", "D", "Eb", "F", "G", "Ab", "Bb"] },
                  "y": { "label": "movement", "tool": "set_bass_line", "arg": "movement", "state": "bass_movement", "values": BASS_MOVES } },
                { "agent": 2,
                  "x": { "label": "mood", "tool": "set_lead_mood", "arg": "mood", "state": "lead", "values": LEAD_MOODS },
                  "y": { "label": "randomness", "tool": "set_lead_randomness", "arg": "percent", "state": "lead_randomness", "min": 0, "max": 100, "step": 5 },
                  "choice": { "label": "sound", "tool": "set_lead_sound", "arg": "instrument", "state": "lead_sound", "values": LEAD_SOUNDS } },
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
            n if n >= FIRST_DRONE_AGENT => drone_brief((n - FIRST_DRONE_AGENT) as usize),
            _ => json!({
                "system": "You are the lead melody player. Only act on requests about the melody or lead.",
                "aliases": ["lead", "melody", "solo", "synth", "randomness"],
                "tools": [
                    function("set_lead_mood", "Change how the lead melody feels.",
                        json!({ "mood": { "type": "string", "enum": LEAD_MOODS } }), &["mood"]),
                    function("set_lead_randomness", "Set how random or unpredictable the lead melody is, as a percent from 0 to 100.",
                        json!({ "percent": { "type": "integer" } }), &["percent"]),
                    // This wording is deliberate: with "sound" in the description or as the
                    // argument name, the model stops answering mood cues such as "lead goes jazz".
                    function("set_lead_sound", "Switch the lead to another instrument.",
                        json!({ "instrument": { "type": "string", "enum": LEAD_SOUNDS } }), &["instrument"]),
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
        if agent >= FIRST_DRONE_AGENT {
            return apply_to_drone((agent - FIRST_DRONE_AGENT) as usize, name, args);
        }
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
                (2, "set_lead_randomness") => {
                    let percent = args["percent"].as_u64().ok_or("`percent` must be a whole number")?;
                    if percent > 100 {
                        return Err(format!("`percent` must be between 0 and 100, got {percent}"));
                    }
                    let changed = b.lead_randomness != percent as u32;
                    b.lead_randomness = percent as u32;
                    Ok(outcome(changed, format!("lead randomness {percent}%")))
                }
                (2, "set_lead_sound") => {
                    let changed = set(&mut b.lead_sound, pick(&LEAD_SOUNDS, &args["instrument"], "instrument")?);
                    Ok(outcome(changed, format!("lead sound {}", b.lead_sound)))
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
            lead(b.lead, b.lead_randomness, root, tick, &mut notes);
            drones(root, &mut notes);
            json!({ "bar": tick, "tempo": b.tempo, "lead_sound": b.lead_sound, "notes": notes }).to_string()
        })
    }

    fn state() -> String {
        BAND.with(|b| {
            let b = b.borrow();
            let mut state = json!({
                "tempo": b.tempo,
                "drums": b.drums,
                "bass_root": b.bass_root,
                "bass_movement": b.bass_move,
                "lead": b.lead,
                "lead_randomness": b.lead_randomness,
                "lead_sound": b.lead_sound,
                "key": "C minor",
            });
            DRONES.with(|d| {
                let d = d.borrow();
                state["drone_count"] = json!(d.len());
                for (i, drone) in d.iter().enumerate() {
                    let n = i + 1;
                    state[format!("drone{n}_interval")] = json!(drone.interval);
                    state[format!("drone{n}_detune")] = json!(drone.detune);
                    state[format!("drone{n}_pulse")] = json!(drone.pulse);
                }
            });
            state.to_string()
        })
    }
}

export!(Component);
