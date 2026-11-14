//! `echo` plugin agent: returns the text it receives. The smallest plugin
//! there is, and a quick check that the harness is wired up.

wit_bindgen::generate!({
    world: "plugin-module",
    path: "../../../../wit/world.wit",
});

use agentwos::core::types::{CallError, Message};
use exports::agentwos::core::agent::Guest;

struct Component;

impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        Ok(input.last().map(|m| m.content.clone()).unwrap_or_default())
    }
}

export!(Component);
