//! Router agent of the CLI harness.
//!
//! `/<plugin> <input>` calls a plugin directly. Anything else goes to the
//! model, which may call plugins one after another before it answers. The
//! plugins arrive through the ordinary `tool` import: the host resolves it to
//! the harness plugin registry, so the router knows nothing about any of them.

wit_bindgen::generate!({
    world: "agent-module",
    path: "../../../wit/world.wit",
});

use agentwos::core::llm;
use agentwos::core::tool;
use agentwos::core::types::{CallError, Message, ToolCall};
use exports::agentwos::core::agent::Guest;
use serde_json::{json, Value};

/// Plugin calls the model may chain for one request.
const MAX_STEPS: usize = 6;
/// How much of a plugin's result is echoed in the trace.
const TRACE_CHARS: usize = 400;

struct Component;

fn text_of(e: CallError) -> String {
    match e {
        CallError::InvalidInput(s) | CallError::ExecutionFailed(s) | CallError::NotFound(s) => s,
    }
}

fn message(role: &str, content: impl Into<String>) -> Message {
    Message { role: role.into(), content: content.into() }
}

/// One "- name: description" line per plugin.
fn plugin_list() -> String {
    let described: Value = serde_json::from_str(&tool::describe()).unwrap_or(Value::Null);
    described["tools"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| format!("- {}: {}", t["name"].as_str().unwrap_or("?"), t["description"].as_str().unwrap_or("")))
        .collect::<Vec<_>>()
        .join("\n")
}

fn call(name: &str, input: &str) -> Result<String, CallError> {
    tool::invoke(&ToolCall { name: name.into(), args_json: json!({ "input": input }).to_string() })
}

fn help() -> String {
    format!(
        "Plugins:\n{}\n\n\
         /<plugin> <input>   call a plugin yourself\n\
         <anything else>     the model decides which plugins to call\n\
         /help               this text\n\
         /exit               leave the prompt",
        plugin_list()
    )
}

/// The first JSON object in a model reply, which may come wrapped in prose or a code fence.
fn json_object(reply: &str) -> Option<Value> {
    let (start, end) = (reply.find('{')?, reply.rfind('}')?);
    serde_json::from_str(reply.get(start..=end)?).ok()
}

fn indent(text: &str) -> String {
    let shown: String = text.trim_end().chars().take(TRACE_CHARS).collect();
    let cut = if text.trim_end().chars().count() > TRACE_CHARS { "\n  ..." } else { "" };
    format!("  {}{cut}", shown.replace('\n', "\n  "))
}

/// Lets the model work towards an answer, one plugin call per step.
fn ask_model(model: &str, request: &str) -> Result<String, CallError> {
    let system = format!(
        "You are the router of a command-line harness. You can call these plugins:\n{}\n\n\
         Reply with one JSON object and nothing else.\n\
         To call a plugin: {{\"plugin\": \"<name>\", \"input\": \"<text for the plugin>\"}}\n\
         To answer the user: {{\"answer\": \"<text>\"}}\n\
         Call one plugin at a time. You then get its result and can call another or answer.",
        plugin_list()
    );
    let mut messages = vec![message("system", system), message("user", request)];
    let mut trace = String::new();

    for _ in 0..MAX_STEPS {
        let reply = llm::complete(&messages, model).map_err(|e| {
            CallError::ExecutionFailed(format!(
                "the model '{model}' did not answer ({}). /<plugin> <input> still works without it.",
                text_of(e)
            ))
        })?;
        // A reply that is not the JSON we asked for is taken as the answer.
        let Some(step) = json_object(&reply) else { return Ok(trace + reply.trim()) };
        if let Some(answer) = step["answer"].as_str() {
            return Ok(trace + answer);
        }
        let Some(name) = step["plugin"].as_str() else { return Ok(trace + reply.trim()) };
        let input = step["input"].as_str().unwrap_or("");

        let result = call(name, input).unwrap_or_else(|e| format!("error: {}", text_of(e)));
        trace += &format!("[{name}] {}\n{}\n", input.lines().next().unwrap_or(""), indent(&result));
        messages.push(message("assistant", reply));
        messages.push(message("user", format!("Result of {name}:\n{result}")));
    }
    Ok(trace + &format!("Stopped after {MAX_STEPS} plugin calls without a final answer."))
}

impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let last = input.last().map(|m| m.content.trim()).unwrap_or("");
        // The harness sends the model as a `model:<id> ` prefix, like `agent-core`.
        let (model, text) = match last.strip_prefix("model:") {
            Some(rest) => rest.split_once(' ').unwrap_or((rest, "")),
            None => ("", last),
        };
        let text = text.trim();

        if text.is_empty() || text == "/help" {
            return Ok(help());
        }
        if let Some(direct) = text.strip_prefix('/') {
            let (name, input) = direct.split_once(char::is_whitespace).unwrap_or((direct, ""));
            return call(name, input.trim());
        }
        ask_model(model, text)
    }
}

export!(Component);
