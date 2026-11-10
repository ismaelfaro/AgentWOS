wit_bindgen::generate!({
    world: "agent-module",
    path: "wit/world.wit",
});

use agentmodules::core::llm;
use agentmodules::core::tool;
use agentmodules::core::types::{CallError, Message, ToolCall};
use exports::agentmodules::core::agent::Guest;
use serde_json::json;

struct Component;

/// Orquestacion minima: si el ultimo mensaje user empieza con "tool:<nombre> ",
/// invoca esa tool; si no, delega al llm. Esto es el loop base; en fases
/// posteriores un modelo real decide que tool llamar (tool-calling del LLM).
impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let last = input
            .last()
            .ok_or_else(|| CallError::InvalidInput("input vacio".into()))?;

        if let Some(rest) = last.content.strip_prefix("tool:echo ") {
            let call = ToolCall {
                name: "echo".into(),
                args_json: json!({ "text": rest }).to_string(),
            };
            return tool::invoke(&call);
        }

        let (model, msgs) = match last.content.strip_prefix("model:") {
            Some(rest) => {
                let (m, text) = rest.split_once(' ').unwrap_or((rest, ""));
                let mut msgs = input.clone();
                msgs.last_mut().unwrap().content = text.to_string();
                (m.to_string(), msgs)
            }
            None => ("openrouter/anthropic/claude-sonnet-4.5".to_string(), input.clone()),
        };
        llm::complete(&msgs, &model)
    }
}

export!(Component);
