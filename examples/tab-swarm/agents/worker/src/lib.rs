wit_bindgen::generate!({
    world: "tool-module",
    path: "../../../../wit/world.wit",
});

use exports::agentwos::core::tool::{CallError, Guest, ToolCall};
use serde_json::{json, Value};

struct Component;

/// Largest range one call may cover, so a single intent cannot occupy a tab for long.
const MAX_SPAN: u64 = 50_000_000;

/// Counts primes in `[from, to)` with a segmented sieve: sieve the small
/// primes up to sqrt(to) once, then cross out their multiples in the range.
fn count_primes(from: u64, to: u64) -> u64 {
    if to <= from || to <= 2 {
        return 0;
    }
    let from = from.max(2);
    let limit = (to as f64).sqrt() as u64 + 1;
    let mut small = vec![true; limit as usize + 1];
    let mut primes = Vec::new();
    for n in 2..=limit {
        if small[n as usize] {
            primes.push(n);
            let mut m = n * n;
            while m <= limit {
                small[m as usize] = false;
                m += n;
            }
        }
    }
    let mut is_prime = vec![true; (to - from) as usize];
    for p in primes {
        let mut m = (p * p).max(from.div_ceil(p) * p);
        while m < to {
            is_prime[(m - from) as usize] = false;
            m += p;
        }
    }
    is_prime.iter().filter(|x| **x).count() as u64
}

impl Guest for Component {
    fn describe() -> String {
        json!({
            "name": "primes.count",
            "description": "Counts the prime numbers in a range.",
            "json_schema": {
                "type": "object",
                "properties": { "from": { "type": "integer" }, "to": { "type": "integer" } },
                "required": ["from", "to"]
            }
        })
        .to_string()
    }

    fn invoke(call: ToolCall) -> Result<String, CallError> {
        if call.name != "primes.count" {
            return Err(CallError::NotFound(format!("this agent does not serve '{}'", call.name)));
        }
        let args: Value = serde_json::from_str(&call.args_json)
            .map_err(|e| CallError::InvalidInput(e.to_string()))?;
        let bound = |key: &str| {
            args[key].as_u64().ok_or_else(|| CallError::InvalidInput(format!("`{key}` must be a whole number")))
        };
        let (from, to) = (bound("from")?, bound("to")?);
        if to < from || to - from > MAX_SPAN {
            return Err(CallError::InvalidInput(format!("range must be at most {MAX_SPAN} numbers")));
        }
        Ok(json!({ "from": from, "to": to, "count": count_primes(from, to) }).to_string())
    }
}

export!(Component);
