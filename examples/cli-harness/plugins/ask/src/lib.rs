//! `ask` plugin agent: sends a question to a model and returns the answer.
//!
//! It needs the `llm` grant. The API key and the network stay in the host's
//! LLM client; this component only sees the `llm` import.

wit_bindgen::generate!({
    world: "plugin-module",
    path: "../../../../wit/world.wit",
});

use agentwos::core::llm;
use agentwos::core::types::{CallError, Message};
use exports::agentwos::core::agent::Guest;

struct Component;

impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let text = input.last().map(|m| m.content.trim()).unwrap_or("");
        // `model:<id> question` picks a model; an empty model means the harness's own.
        let (model, question) = match text.strip_prefix("model:") {
            Some(rest) => rest.split_once(' ').unwrap_or((rest, "")),
            None => ("", text),
        };
        if question.trim().is_empty() {
            return Ok("usage: ask [model:<id>] <question>".into());
        }
        let messages = [
            Message { role: "system".into(), content: "Answer directly and briefly.".into() },
            Message { role: "user".into(), content: question.trim().into() },
        ];
        llm::complete(&messages, model)
    }
}

export!(Component);
