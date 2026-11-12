mod control;
mod runner;
mod runtime;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Result};
use clap::Parser;
use serde_json::Value;
use wasmtime::{Config, Engine};

use runtime::{agent_message, AgentInstance, Components};

const DEFAULT_MODEL: &str = "local/google/gemma-4-e4b";

#[derive(Parser)]
#[command(about = "Host runtime for agents, tools, LLM clients, scenarios and OS controls packaged as WASM components")]
struct Args {
    /// Manifest JSON (scenario, controls, model, days, log). Relative paths resolve from its folder.
    #[arg(long)]
    manifest: Option<PathBuf>,

    /// Scenario WASM component (overrides the manifest).
    #[arg(long)]
    scenario: Option<String>,

    /// OS control WASM component (repeatable; overrides the manifest).
    #[arg(long = "control")]
    controls: Vec<String>,

    /// Model for scenario runs (local/ or openrouter/ prefix).
    #[arg(long)]
    model: Option<String>,

    /// Run the scenario for N days.
    #[arg(long)]
    colony: Option<u32>,

    /// Dashboard log (window.SIM_DATA), rewritten after every turn.
    #[arg(long)]
    log: Option<PathBuf>,

    /// Agent WASM component for single-agent runs.
    #[arg(long, default_value = "modules/agent-core/target/wasm32-wasip2/release/agent_core.wasm")]
    agent: String,

    /// Tool WASM component for single-agent runs.
    #[arg(long, default_value = "modules/tool-echo/target/wasm32-wasip2/release/tool_echo.wasm")]
    tool: String,

    /// LLM client WASM component.
    #[arg(long, default_value = "modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm")]
    llm: String,

    /// Message for a single-agent run.
    #[arg(long, default_value = "hello agent")]
    message: String,
}

#[derive(Default)]
struct Manifest {
    scenario: Option<String>,
    controls: Vec<String>,
    model: Option<String>,
    days: Option<u32>,
    log: Option<PathBuf>,
}

impl Manifest {
    fn load(path: &Path) -> Result<Self> {
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|e| anyhow!("manifest {}: {e}", path.display()))?;
        let base = path.parent().unwrap_or(Path::new("."));
        let resolve = |p: &str| base.join(p).to_string_lossy().into_owned();
        Ok(Self {
            scenario: raw["scenario"].as_str().map(resolve),
            controls: raw["controls"]
                .as_array()
                .map(|a| a.iter().filter_map(Value::as_str).map(resolve).collect())
                .unwrap_or_default(),
            model: raw["model"].as_str().map(str::to_string),
            days: raw["days"].as_u64().map(|d| d as u32),
            log: raw["log"].as_str().map(|p| base.join(p)),
        })
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let manifest = match &args.manifest {
        Some(p) => Manifest::load(p)?,
        None => Manifest::default(),
    };

    let scenario = args.scenario.or(manifest.scenario);
    let controls = if args.controls.is_empty() { manifest.controls } else { args.controls };
    let model = args.model.or(manifest.model).unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let days = args.colony.or(manifest.days);
    let log = args.log.or(manifest.log);

    let mut config = Config::new();
    config.wasm_component_model(true);
    let comps = Components::load(
        Engine::new(&config)?,
        &args.agent,
        &args.tool,
        &args.llm,
        scenario.as_deref(),
        &controls,
    )?;

    if let Some(days) = days {
        return runner::run(&comps, days, &model, log.as_deref());
    }

    let mut agent = AgentInstance::spawn(&comps)?;
    match agent.ask(vec![agent_message("user".into(), args.message)]) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("{e}"),
    }
    Ok(())
}
