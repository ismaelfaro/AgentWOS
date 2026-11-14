//! Tab Jam stage: the band's shared state and the notes of each bar.
//!
//! This file only routes the `stage` interface to the agents. Each agent lives
//! in its own module with the same shape: `describe`, `brief`, `pad`, `apply`,
//! `state` and `render`.
//!
//! - `drummer`   drum pattern and the band's tempo
//! - `bassist`   bass line and the root everyone tunes to
//! - `lead`      melody mood, randomness and synth voice
//! - `orchestra` sections added one by one while playing
//! - `music`     notes, scales and helpers shared by all of them

wit_bindgen::generate!({
    world: "stage-module",
    path: "../../../wit/world.wit",
});

mod bassist;
mod drummer;
mod lead;
mod music;
mod orchestra;

use exports::agentwos::core::stage::Guest;
use serde_json::{json, Value};

struct Component;

/// Agent indexes. The band is fixed; orchestra sections follow from `ORCHESTRA`.
const DRUMMER: u32 = 0;
const BASSIST: u32 = 1;
const LEAD: u32 = 2;
const ORCHESTRA: u32 = 3;

impl Guest for Component {
    fn setup(_config: String) -> String {
        let suggestions: Vec<&str> = drummer::SUGGESTIONS
            .into_iter()
            .chain(bassist::SUGGESTIONS)
            .chain(lead::SUGGESTIONS)
            .chain(drummer::TEMPO_SUGGESTIONS)
            .chain(["Silence the drums and make the melody bright"])
            .collect();
        json!({
            "agents": [drummer::describe(), bassist::describe(), lead::describe()],
            "key": music::KEY,
            "steps_per_bar": music::STEPS_PER_BAR,
            "suggestions": suggestions,
            // Touch pads for the UI: two settings per agent, each mapped to one
            // tool argument and to the state field that shows its current value.
            "pads": [
                drummer::pad(DRUMMER as usize),
                bassist::pad(BASSIST as usize),
                lead::pad(LEAD as usize),
            ],
        })
        .to_string()
    }

    fn spawn(kind: String) -> Result<String, String> {
        if kind != "drone" {
            return Err(format!("this stage can only add an orchestra 'drone', not '{kind}'"));
        }
        let agent = ORCHESTRA as usize + orchestra::count();
        orchestra::join(agent).map(|description| description.to_string())
    }

    fn brief(agent: u32) -> String {
        match agent {
            DRUMMER => drummer::brief(),
            BASSIST => bassist::brief(),
            LEAD => lead::brief(),
            n => orchestra::brief((n - ORCHESTRA) as usize),
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
        match agent {
            DRUMMER => drummer::apply(name, args),
            BASSIST => bassist::apply(name, args),
            LEAD => lead::apply(name, args),
            n => orchestra::apply((n - ORCHESTRA) as usize, name, args),
        }
    }

    fn render(tick: u32) -> String {
        let root = bassist::root();
        let mut notes = Vec::new();
        drummer::render(tick, &mut notes);
        bassist::render(&mut notes);
        lead::render(root, tick, &mut notes);
        orchestra::render(root, &mut notes);
        json!({ "bar": tick, "tempo": drummer::tempo(), "lead_sound": lead::sound(), "notes": notes }).to_string()
    }

    fn state() -> String {
        let mut state = json!({});
        drummer::state(&mut state);
        bassist::state(&mut state);
        lead::state(&mut state);
        state["key"] = json!(music::KEY);
        orchestra::state(&mut state);
        // Who follows whom. `state` names the field that carries the signal, so a
        // UI can show it travelling when that field changes.
        let everyone = 0..ORCHESTRA + orchestra::count() as u32;
        state["links"] = json!([
            { "signal": "tempo", "state": "tempo", "from": DRUMMER,
              "to": everyone.clone().filter(|a| *a != DRUMMER).collect::<Vec<_>>() },
            { "signal": "root", "state": "bass_root", "from": BASSIST,
              "to": everyone.filter(|a| *a >= LEAD).collect::<Vec<_>>() },
        ]);
        state.to_string()
    }
}

export!(Component);
