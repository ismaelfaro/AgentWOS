//! `wordcount` plugin agent: counts words, characters and lines. It uses no
//! host capability at all.

wit_bindgen::generate!({
    world: "plugin-module",
    path: "../../../../wit/world.wit",
});

use agentwos::core::types::{CallError, Message};
use exports::agentwos::core::agent::Guest;

struct Component;

impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let text = input.last().map(|m| m.content.as_str()).unwrap_or("");
        Ok(format!(
            "words: {}\ncharacters: {}\nlines: {}",
            text.split_whitespace().count(),
            text.chars().count(),
            text.lines().count()
        ))
    }
}

export!(Component);
