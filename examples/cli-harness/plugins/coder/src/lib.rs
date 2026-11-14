//! `coder` plugin agent: given a task, it writes files, runs them and fixes
//! what fails, until it is done.
//!
//! It needs both grants: `llm` to think and `workspace` to act. The model
//! proposes one step at a time and the host applies it, so nothing happens
//! outside the workspace or with a program the manifest does not allow.

wit_bindgen::generate!({
    world: "plugin-module",
    path: "../../../../wit/world.wit",
});

use agentwos::core::llm;
use agentwos::core::types::{CallError, Message};
use agentwos::core::workspace;
use exports::agentwos::core::agent::Guest;
use serde_json::Value;

/// Steps the model may take for one task.
const MAX_STEPS: usize = 8;
/// How much of each step's result is echoed in the trace.
const TRACE_CHARS: usize = 300;

const SYSTEM: &str = "\
You write and run code in a workspace folder. Reply with one JSON object and nothing else:
{\"write\": {\"path\": \"<relative path>\", \"content\": \"<whole file>\"}}
{\"run\": {\"program\": \"<program>\", \"args\": [\"<arg>\", \"...\"]}}
{\"read\": \"<relative path>\"}
{\"done\": \"<what you did and what the result was>\"}
Take one step at a time; you get the result of each. When a run fails, fix the file and run again. \
Only a few programs may be run; if one is refused, the error lists the allowed ones.";

struct Component;

fn text_of(e: CallError) -> String {
    match e {
        CallError::InvalidInput(s) | CallError::ExecutionFailed(s) | CallError::NotFound(s) => s,
    }
}

fn message(role: &str, content: impl Into<String>) -> Message {
    Message { role: role.into(), content: content.into() }
}

/// Every JSON object in the reply, in order. A small model often sends several
/// steps at once (write, then run) or wraps them in a code fence.
fn json_objects(reply: &str) -> Vec<Value> {
    let Some(start) = reply.find('{') else { return Vec::new() };
    serde_json::Deserializer::from_str(&reply[start..]).into_iter().map_while(Result::ok).collect()
}

/// A step's result as it appears in the trace: indented and cut short.
fn indent(text: &str) -> String {
    let shown: String = text.chars().take(TRACE_CHARS).collect();
    let cut = if text.chars().count() > TRACE_CHARS { "\n  ..." } else { "" };
    format!("  {}{cut}", shown.replace('\n', "\n  "))
}

/// Applies one step. Returns a short label for the trace and the result the model sees.
fn apply(step: &Value) -> Result<(String, String), String> {
    if let Some(write) = step.get("write") {
        let path = write["path"].as_str().ok_or("`write` needs a `path`")?;
        let content = write["content"].as_str().ok_or("`write` needs `content`")?;
        let result = workspace::write(path, content).map_err(text_of)?;
        return Ok((format!("write {path}"), result));
    }
    if let Some(run) = step.get("run") {
        let program = run["program"].as_str().ok_or("`run` needs a `program`")?;
        let args: Vec<String> =
            run["args"].as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect();
        let result = workspace::exec(program, &args).map_err(text_of)?;
        let output = format!("exit {}\n{}{}", result.status, result.stdout, result.stderr);
        return Ok((format!("run {program} {}", args.join(" ")), output.trim_end().to_string()));
    }
    if let Some(path) = step.get("read").and_then(Value::as_str) {
        return Ok((format!("read {path}"), workspace::read(path).map_err(text_of)?));
    }
    Err("the reply must have one of: write, run, read, done".into())
}

impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let task = input.last().map(|m| m.content.trim()).unwrap_or("");
        if task.is_empty() {
            return Ok("usage: coder <what to build and run>".into());
        }
        let mut messages = vec![message("system", SYSTEM), message("user", task)];
        let mut trace = String::new();

        for _ in 0..MAX_STEPS {
            // An empty model name means the harness's own model.
            let reply = llm::complete(&messages, "")?;
            let mut steps = json_objects(&reply);
            if steps.is_empty() {
                steps.push(Value::Null);
            }
            let mut results = String::new();
            for step in &steps {
                if let Some(done) = step.get("done").and_then(Value::as_str) {
                    return Ok(trace + done);
                }
                let (label, result) = match apply(step) {
                    Ok(applied) => applied,
                    Err(problem) => ("error".to_string(), problem),
                };
                trace += &format!("{label}\n{}\n", indent(&result));
                results += &format!("{label}:\n{result}\n");
            }
            messages.push(message("assistant", reply));
            messages.push(message("user", format!("Result:\n{results}")));
        }
        Ok(trace + &format!("Stopped after {MAX_STEPS} steps without finishing."))
    }
}

export!(Component);
