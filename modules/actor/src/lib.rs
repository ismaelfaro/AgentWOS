wit_bindgen::generate!({
    world: "actor-module",
    path: "../../wit/world.wit",
});

use agentwos::core::tool_llm;
use agentwos::core::types::CallError;
use exports::agentwos::core::actor::Guest;
use serde_json::{json, Value};

struct Component;

const DEFAULT_MIN_CONFIDENCE: f64 = 0.15;

/// Small tool-calling models handle one intent at a time, so a compound
/// instruction is split into clauses and each is asked separately.
fn clauses(input: &str) -> Vec<String> {
    let mut text = input.to_string();
    for sep in [" and then ", " then ", " and ", ";", ","] {
        text = text.replace(sep, "\n");
    }
    text.lines()
        .flat_map(|l| l.split(". "))
        .map(|c| c.trim().trim_end_matches('.').to_string())
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

        let mut attempts = Vec::new();
        for clause in clauses(&input) {
            if !aliases.is_empty() && !addressed(&clause, &aliases) {
                continue;
            }
            let raw = tool_llm::call(system, &tools, &clause)?;
            let reply: Value = serde_json::from_str(&raw)
                .map_err(|e| CallError::ExecutionFailed(format!("llm reply is not JSON: {e}")))?;
            let confidence = reply["confidence"].as_f64().unwrap_or(0.0);
            let calls = reply["function_calls"].as_array().cloned().unwrap_or_default();
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
