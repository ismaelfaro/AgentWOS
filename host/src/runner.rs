use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde_json::{json, Map, Value};

use crate::control::ControlChain;
use crate::runtime::{AgentInstance, Components, ScenarioInstance};

/// Dashboard log, rewritten atomically (tmp + rename) after every turn so the
/// dashboard never reads a half-written file.
struct Log {
    path: Option<PathBuf>,
    header: Map<String, Value>,
}

impl Log {
    fn write(&self, days: &str, finished: bool) -> Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        let mut data = self.header.clone();
        data.insert("finished".into(), json!(finished));
        data.insert("days".into(), serde_json::from_str(days)?);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("js.tmp");
        std::fs::write(&tmp, format!("window.SIM_DATA = {};\n", Value::Object(data)))?;
        std::fs::rename(tmp, path)?;
        Ok(())
    }
}

/// Drives a scenario: the scenario decides prompts, rules and state; the kernel
/// runs the agents, passes every action through the controls and logs the result.
pub fn run(c: &Components, days: u32, model: &str, log_path: Option<&Path>) -> Result<()> {
    let component = c.scenario.as_ref().ok_or_else(|| anyhow!("no scenario loaded"))?;
    let mut scenario = ScenarioInstance::spawn(c, component)?;
    let description: Value = serde_json::from_str(&scenario.setup(&json!({ "model": model, "days": days }).to_string())?)?;

    let agents: Vec<(String, String)> = description["agents"]
        .as_array()
        .ok_or_else(|| anyhow!("scenario setup must return an `agents` array"))?
        .iter()
        .map(|a| (a["agent"].as_str().unwrap_or("").to_string(), a["role"].as_str().unwrap_or("").to_string()))
        .collect();
    let mut instances = agents.iter().map(|_| AgentInstance::spawn(c)).collect::<Result<Vec<_>>>()?;
    let mut controls = ControlChain::spawn(c)?;

    let mut header = Map::new();
    header.insert("model".into(), json!(model));
    header.insert("days_total".into(), json!(days));
    header.insert("start_unix".into(), json!(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs()));
    header.insert("controls".into(), json!(controls.names()));
    if let Value::Object(fields) = description {
        header.extend(fields);
    }
    let log = Log { path: log_path.map(Path::to_path_buf), header };
    log.write(&scenario.snapshot()?, false)?;

    println!("# Simulation: {days} days, model {model}, controls {:?}\n", controls.names());

    for day in 1..=days {
        let event = scenario.begin_day(day, days)?;
        println!("## Day {day}: {event}");
        log.write(&scenario.snapshot()?, false)?;

        for (i, (name, _role)) in agents.iter().enumerate() {
            let messages = scenario.turn_messages(i as u32)?;

            let t0 = Instant::now();
            let reply = instances[i].ask(messages);
            let latency_ms = t0.elapsed().as_millis() as u64;

            let (raw, error) = match reply {
                Ok(text) => (text, None),
                Err(e) => (String::new(), Some(e)),
            };
            let action = scenario.action_of(i as u32, &raw)?;
            let context = scenario.context()?;
            let outcome = controls.apply(name, action, &context);

            scenario.record_turn(
                i as u32,
                outcome.delivered.as_deref(),
                &Value::Array(outcome.records).to_string(),
                latency_ms,
                error.as_deref(),
            )?;
            match &outcome.delivered {
                Some(text) => println!("**{name}**: {text}  ({latency_ms} ms)"),
                None => println!("**{name}**: BLOCKED ({latency_ms} ms)"),
            }
            log.write(&scenario.snapshot()?, false)?;
        }

        let collapsed = scenario.end_day()?;
        log.write(&scenario.snapshot()?, false)?;
        println!("End of day {day}\n");
        if collapsed {
            println!("ALERT: scenario collapsed on day {day}.");
            break;
        }
    }

    log.write(&scenario.snapshot()?, true)
}
