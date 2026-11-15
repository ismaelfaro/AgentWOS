wit_bindgen::generate!({
    world: "agent-module",
    path: "../../../../wit/world.wit",
});

use agentwos::core::types::{CallError, Message};
use exports::agentwos::core::agent::Guest;
use serde_json::{json, Value};

struct Component;

const MAX_PIECES: u64 = 48;

/// The lead agent of the swarm. It never computes anything itself: it turns a
/// request into pieces of work for a pool, and the pieces' results into an
/// answer. The kernel carries the pieces to other realms between the two calls.
///
/// Protocol with the kernel, one JSON object per reply:
///   {"ask": {"pool": selector, "verb": "...", "bodies": [...]}}   work to send
///   {"done": "text"}                                              final answer
/// The kernel answers an `ask` with a `tool` message holding the results.
impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let last = input.last().ok_or_else(|| CallError::InvalidInput("no messages".into()))?;
        let members = input
            .iter()
            .find(|m| m.role == "system")
            .and_then(|m| serde_json::from_str::<Value>(&m.content).ok())
            .and_then(|v| v["members"].as_u64())
            .unwrap_or(0);

        if last.role == "tool" {
            let results: Value = serde_json::from_str(&last.content)
                .map_err(|e| CallError::InvalidInput(format!("results are not JSON: {e}")))?;
            let pieces = results.as_array().map(Vec::len).unwrap_or(0);
            let total: u64 = results.as_array().into_iter().flatten().filter_map(|r| r["count"].as_u64()).sum();
            let top = results.as_array().into_iter().flatten().filter_map(|r| r["to"].as_u64()).max().unwrap_or(0);
            return Ok(json!({
                "done": format!("There are {total} primes below {top}, counted in {pieces} pieces."),
                "total": total,
            })
            .to_string());
        }

        let limit = last
            .content
            .split_whitespace()
            .filter_map(|w| w.replace(['_', ','], "").parse::<u64>().ok())
            .next_back()
            .ok_or_else(|| CallError::InvalidInput("say a number, for example: primes 5000000".into()))?;
        if members == 0 {
            return Ok(json!({ "done": "No worker is connected. Open the page in a browser first." }).to_string());
        }

        // Two pieces per member, so a slow member does not hold half of the work.
        let pieces = (members * 2).clamp(1, MAX_PIECES).min(limit.max(1));
        let size = limit.div_ceil(pieces);
        let bodies: Vec<Value> = (0..pieces)
            .map(|i| json!({ "from": i * size, "to": ((i + 1) * size).min(limit) }))
            .filter(|b| b["from"].as_u64() < b["to"].as_u64())
            .collect();
        Ok(json!({ "ask": {
            "pool": { "labels": { "role": "worker" } },
            "verb": "primes.count",
            "bodies": bodies,
        }})
        .to_string())
    }
}

export!(Component);
