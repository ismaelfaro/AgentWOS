wit_bindgen::generate!({
    world: "agent-module",
    path: "../../wit/world.wit",
});

use agentmodules::core::llm;
use agentmodules::core::tool;
use agentmodules::core::types::{CallError, Message, ToolCall};
use exports::agentmodules::core::agent::Guest;
use serde_json::json;

struct Component;

/// Minimal orchestration: if the last user message starts with "tool:echo ",
/// call that tool; otherwise delegate to the LLM. A "model:<id> " prefix picks
/// the model for that message.
impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let last = input
            .last()
            .ok_or_else(|| CallError::InvalidInput("empty input".into()))?;

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
