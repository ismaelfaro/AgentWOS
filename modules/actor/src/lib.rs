wit_bindgen::generate!({
    world: "actor-module",
    path: "../../wit/world.wit",
});

use agentwos::core::tool_llm;
use agentwos::core::types::CallError;
use exports::agentwos::core::actor::Guest;
use serde_json::{json, Value};

struct Component;

/// The engine already withholds calls it scores below its own floor, so this
/// only guards against engines that do not.
const DEFAULT_MIN_CONFIDENCE: f64 = 0.1;

/// Small tool-calling models handle one intent at a time, so a compound
/// instruction is split into clauses and each is asked separately.
fn clauses(input: &str) -> Vec<String> {
    let mut text = input.to_string();
    for sep in [" and then ", " then ", " and ", ";", ","] {
        text = text.replace(sep, "\n");
    }
    text.lines()
        .flat_map(|l| l.split(". "))
        .map(|c| c.trim_matches(|ch: char| ch.is_whitespace() || "\"'.!?".contains(ch)).to_string())
        .filter(|c| !c.is_empty())
        .collect()
}

/// A clause is addressed to this agent when it mentions one of its aliases.
/// Without this gate a small model grabs any clause that fits one of its enums.
fn addressed(clause: &str, aliases: &[String]) -> bool {
    let lower = clause.to_lowercase();
    let words: Vec<&str> = lower.split(|c: char| !c.is_alphanumeric()).collect();
    aliases.iter().any(|a| words.iter().any(|w| w == a))
}

/// The clause without the words that only say who it is for ("Pip attack" ->
/// "attack"). A small model can take a name for part of the request.
fn without_aliases(clause: &str, aliases: &[String]) -> String {
    let kept: Vec<&str> = clause
        .split_whitespace()
        .filter(|w| {
            let bare = w.trim_matches(|c: char| !c.is_alphanumeric()).to_lowercase();
            !aliases.iter().any(|a| *a == bare)
        })
        .collect();
    if kept.is_empty() { clause.to_string() } else { kept.join(" ") }
}

/// Names of the required parameters of `tool_name` in an OpenAI-style tool list.
fn required_params(tools: &Value, tool_name: &str) -> Vec<String> {
    tools
        .as_array()
        .into_iter()
        .flatten()
        .find(|t| t["function"]["name"].as_str() == Some(tool_name))
        .and_then(|t| t["function"]["parameters"]["required"].as_array())
        .map(|r| r.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// Small models fill optional arguments with plausible values the user never
/// asked for. An optional text argument is kept only when the clause mentions it.
fn drop_ungrounded(call: &mut Value, clause: &str, tools: &Value) {
    let required = required_params(tools, call["name"].as_str().unwrap_or(""));
    let clause = clause.to_lowercase();
    if let Some(args) = call["arguments"].as_object_mut() {
        args.retain(|key, value| {
            let Some(text) = value.as_str() else { return true };
            if required.iter().any(|r| r == key) {
                return true;
            }
            let stem: String = text.to_lowercase().replace('_', " ").chars().take(4).collect();
            !stem.is_empty() && clause.contains(&stem)
        });
    }
}

impl Guest for Component {
    fn act(brief: String, input: String) -> Result<String, CallError> {
        let brief: Value = serde_json::from_str(&brief)
            .map_err(|e| CallError::InvalidInput(format!("brief is not JSON: {e}")))?;
        let system = brief["system"].as_str().unwrap_or("");
        let tools = brief["tools"].to_string();
        let min_confidence = brief["min_confidence"].as_f64().unwrap_or(DEFAULT_MIN_CONFIDENCE);
        let aliases: Vec<String> = brief["aliases"]
            .as_array()
            .map(|a| a.iter().filter_map(Value::as_str).map(str::to_lowercase).collect())
            .unwrap_or_default();

        // Optional: send the model the clause without the alias that addressed it.
        let strip = brief["strip_aliases"].as_bool().unwrap_or(false);

        let mut attempts = Vec::new();
        for clause in clauses(&input) {
            if !aliases.is_empty() && !addressed(&clause, &aliases) {
                continue;
            }
            let asked = if strip { without_aliases(&clause, &aliases) } else { clause.clone() };
            let raw = tool_llm::call(system, &tools, &asked)?;
            let reply: Value = serde_json::from_str(&raw)
                .map_err(|e| CallError::ExecutionFailed(format!("llm reply is not JSON: {e}")))?;
            let confidence = reply["confidence"].as_f64().unwrap_or(0.0);
            let mut calls = reply["function_calls"].as_array().cloned().unwrap_or_default();
            for call in &mut calls {
                drop_ungrounded(call, &clause, &brief["tools"]);
            }
            let ignored = if calls.is_empty() {
                Some("the model proposed no call")
            } else if confidence < min_confidence {
                Some("confidence below the agent's threshold")
            } else {
                None
            };
            attempts.push(json!({
                "clause": clause,
                "confidence": confidence,
                "calls": if ignored.is_some() { Vec::new() } else { calls },
                "ignored": ignored,
                "reasoning": reply["reasoning"],
            }));
        }
        Ok(Value::Array(attempts).to_string())
    }
}

export!(Component);
