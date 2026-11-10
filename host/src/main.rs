mod colony;
mod control;
mod runtime;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use wasmtime::{Config, Engine};

use runtime::{AgentInstance, Components};

#[derive(Parser)]
#[command(about = "Host runtime para agentes/tools/llm empaquetados como componentes WASM")]
struct Args {
    /// Componente WASM del agente.
    #[arg(long, default_value = "modules/agent-core/target/wasm32-wasip2/release/agent_core.wasm")]
    agent: String,

    /// Componente WASM de la tool.
    #[arg(long, default_value = "modules/tool-echo/target/wasm32-wasip2/release/tool_echo.wasm")]
    tool: String,

    /// Componente WASM del llm-call.
    #[arg(long, default_value = "modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm")]
    llm: String,

    /// Componente WASM de control del OS (repetible, se aplican en orden).
    #[arg(long = "control")]
    controls: Vec<String>,

    /// Mensaje para ejecutar un solo agente.
    #[arg(long, default_value = "hola agente")]
    message: String,

    /// Simula la colonia en Marte durante N dias.
    #[arg(long)]
    colony: Option<u32>,

    /// Modelo del modo colonia (prefijo local/ u openrouter/).
    #[arg(long, default_value = "local/google/gemma-4-e4b")]
    model: String,

    /// Log para el dashboard (window.SIM_DATA), reescrito tras cada turno.
    #[arg(long)]
    log: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut config = Config::new();
    config.wasm_component_model(true);
    let comps = Components::load(Engine::new(&config)?, &args.agent, &args.tool, &args.llm, &args.controls)?;

    if let Some(dias) = args.colony {
        return colony::run(&comps, dias, &args.model, args.log.as_deref());
    }

    let mut agente = AgentInstance::spawn(&comps)?;
    match agente.ask(&[("user", args.message)]) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("{e}"),
    }
    Ok(())
}
