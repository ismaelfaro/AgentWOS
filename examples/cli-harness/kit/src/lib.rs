//! What every plugin agent of the harness shares: the bindings of the
//! `plugin-module` world and a few helpers. A plugin is then only its own
//! logic:
//!
//! ```ignore
//! use kit::*;
//! struct Echo;
//! impl Guest for Echo {
//!     fn run(input: Vec<Message>) -> Reply { Ok(text(&input).to_string()) }
//! }
//! kit::export!(Echo);
//! ```

wit_bindgen::generate!({
    world: "plugin-module",
    path: "../../../wit/world.wit",
    pub_export_macro: true,
    default_bindings_module: "kit",
});

pub use agentwos::core::types::{CallError, Message, ToolCall};
pub use agentwos::core::{llm, tool, workspace};
pub use exports::agentwos::core::agent::Guest;
pub use serde_json::{json, Value};

pub type Reply = Result<String, CallError>;

pub fn invalid(message: impl Into<String>) -> CallError {
    CallError::InvalidInput(message.into())
}

pub fn text_of(e: CallError) -> String {
    match e {
        CallError::InvalidInput(s) | CallError::ExecutionFailed(s) | CallError::NotFound(s) => s,
    }
}

pub fn message(role: &str, content: impl Into<String>) -> Message {
    Message { role: role.into(), content: content.into() }
}

/// What the plugin was asked: the last message, trimmed.
pub fn text(input: &[Message]) -> &str {
    input.last().map(|m| m.content.trim()).unwrap_or("")
}

/// The first word of a request and what follows it.
pub fn command(text: &str) -> (&str, &str) {
    let (first, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
    (first, rest.trim())
}

/// Splits a command line into words. Single and double quotes group words, and
/// a backslash keeps the next character as it is. No shell is involved.
pub fn words(line: &str) -> Result<Vec<String>, CallError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('\''), c) => word.push(c),
            (_, '\\') => word.push(chars.next().ok_or_else(|| invalid("a backslash needs a character after it"))?),
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if quote.is_some() {
        return Err(invalid("a quote is never closed"));
    }
    if started || !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

/// Runs a program in the workspace and reports its exit code and output.
/// The host refuses a program this plugin's `allow` list does not name.
pub fn run(program: &str, args: &[String]) -> Reply {
    let result = workspace::exec(program, args)?;
    let mut out = format!("exit {}", result.status);
    for (label, stream) in [("", &result.stdout), ("stderr:\n", &result.stderr)] {
        if !stream.trim().is_empty() {
            out += &format!("\n{label}{}", stream.trim_end());
        }
    }
    Ok(out)
}

/// `run` with fixed leading arguments and the rest taken from a command line.
pub fn run_line(program: &str, leading: &[&str], line: &str) -> Reply {
    let mut args: Vec<String> = leading.iter().map(|a| a.to_string()).collect();
    args.extend(words(line)?);
    run(program, &args)
}

/// The other plugin agents that fit this project, as (name, description).
/// Needs the `plugins` grant; empty without it.
pub fn agents() -> Vec<(String, String)> {
    let described: Value = serde_json::from_str(&tool::describe()).unwrap_or(Value::Null);
    let text = |v: &Value| v.as_str().unwrap_or("").to_string();
    described["tools"].as_array().into_iter().flatten().map(|t| (text(&t["name"]), text(&t["description"]))).collect()
}

/// Asks another plugin agent. Needs the `plugins` grant.
pub fn call(agent: &str, input: &str) -> Reply {
    tool::invoke(&ToolCall { name: agent.into(), args_json: json!({ "input": input }).to_string() })
}

/// Every JSON object in a model reply, in order. A small model often sends
/// several at once, or wraps them in prose or a code fence.
pub fn json_objects(reply: &str) -> Vec<Value> {
    let Some(start) = reply.find('{') else { return Vec::new() };
    serde_json::Deserializer::from_str(&reply[start..]).into_iter().map_while(Result::ok).collect()
}

/// At most `max` characters of a text, saying so when it was cut.
pub fn clip(text: &str, max: usize) -> String {
    match text.char_indices().nth(max) {
        Some((end, _)) => format!("{}\n[cut: {} more characters]", &text[..end], text.len() - end),
        None => text.to_string(),
    }
}
