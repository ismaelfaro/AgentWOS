mod activity;
mod control;
mod dashboard;
mod ledger;
mod harness;
mod a2a;
mod http;
mod mcp;
mod prompt;
mod runner;
mod runtime;
mod workspace;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Result};
use clap::Parser;
use serde_json::Value;
use wasmtime::{Config, Engine};

use runtime::{message, AgentInstance, Components};

const DEFAULT_MODEL: &str = "local/google/gemma-4-e4b";

#[derive(Parser)]
#[command(about = "Host runtime for agents, tools, LLM clients, scenarios and OS controls packaged as WASM components")]
struct Args {
    /// Scenario manifest JSON (scenario, controls, model, days, log). Relative paths resolve from its folder.
    #[arg(long)]
    manifest: Option<PathBuf>,

    /// Harness manifest JSON: a router agent plus plugin agents. With
    /// --message it answers once; without it, it opens a prompt.
    #[arg(long)]
    harness: Option<PathBuf>,

    /// With --harness: list the plugins and exit.
    #[arg(long)]
    list: bool,

    /// With --harness: show every line of what the agents do, also when not
    /// on a terminal, instead of a board for agents that work in parallel.
    #[arg(long)]
    trace: bool,

    /// With --harness: the project folder the plugins work in. Overrides the
    /// manifest's `workspace`.
    #[arg(long)]
    project: Option<PathBuf>,

    /// With --harness: show a server of the manifest's `mcp` section, with its
    /// tools, and pin them if you agree. Agents use no server before this.
    #[arg(long, value_name = "SERVER")]
    mcp_approve: Option<String>,

    /// With --harness: connect every MCP server, print the state and counters
    /// of each sidecar, and exit.
    #[arg(long)]
    mcp_status: bool,

    /// With --harness: call one tool of an MCP server as yourself, with no
    /// model: `--mcp-call server.tool --message '{"arguments": "as JSON"}'`.
    #[arg(long, value_name = "SERVER.TOOL")]
    mcp_call: Option<String>,

    /// With --harness: be an MCP server on stdin and stdout, offering the agents
    /// listed in the manifest's `mcp.serve.expose`.
    #[arg(long)]
    mcp_serve: bool,

    /// With --harness: serve the exposed agents over HTTP on a loopback address
    /// (for example 127.0.0.1:8820): MCP at /mcp, A2A at /a2a, and the Agent Card at
    /// /.well-known/agent-card.json. Needs `mcp.serve.auth.bearer_env`.
    #[arg(long, value_name = "ADDR")]
    serve_http: Option<String>,

    /// With --harness: show what the agents do on a dashboard at this loopback
    /// address (for example 127.0.0.1:8830). The link, with its token, is
    /// printed on stderr.
    #[arg(long, value_name = "ADDR")]
    dashboard: Option<String>,

    /// Scenario WASM component (overrides the manifest).
    #[arg(long)]
    scenario: Option<String>,

    /// OS control WASM component (repeatable; overrides the manifest).
    #[arg(long = "control")]
    controls: Vec<String>,

    /// Model (local/ or openrouter/ prefix). Overrides the manifest.
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

    /// Message for a single-agent or harness run.
    #[arg(long)]
    message: Option<String>,
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
    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    if let Some(path) = &args.harness {
        let mcp = harness::McpOptions { approve: args.mcp_approve, status: args.mcp_status, call: args.mcp_call, serve: args.mcp_serve, serve_http: args.serve_http, dashboard: args.dashboard };
        return harness::run(&engine, path, args.project, args.model.as_deref(), args.message.as_deref(), args.list, args.trace, mcp);
    }
    if args.mcp_approve.is_some() || args.mcp_status || args.mcp_call.is_some() || args.mcp_serve || args.serve_http.is_some() {
        bail!("the --mcp-* options need --harness");
    }
    if args.list || args.project.is_some() || args.dashboard.is_some() {
        bail!("--list, --project and --dashboard need --harness");
    }

    let manifest = match &args.manifest {
        Some(p) => Manifest::load(p)?,
        None => Manifest::default(),
    };
    let scenario = args.scenario.or(manifest.scenario);
    let controls = if args.controls.is_empty() { manifest.controls } else { args.controls };
    let model = args.model.or(manifest.model).unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let comps = Components::load(engine, &args.agent, &args.tool, &args.llm, scenario.as_deref(), &controls)?;

    if let Some(days) = args.colony.or(manifest.days) {
        return runner::run(&comps, days, &model, args.log.or(manifest.log).as_deref());
    }

    let mut agent = AgentInstance::spawn(&comps)?;
    let text = args.message.unwrap_or_else(|| "hello agent".into());
    println!("{}", agent.ask(vec![message("user", text)]).map_err(|e| anyhow!(e))?);
    Ok(())
}
