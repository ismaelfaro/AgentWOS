use std::cell::RefCell;

use anyhow::{anyhow, Result};
use clap::Parser;
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

/// Estado WASI minimo: todos los modulos (agent-core, tool-echo, llm-call)
/// arrastran import de wasi:cli/* porque son binarios std de Rust, aunque
/// no usen stdio/env de verdad. Cada instancia vive en su propio Store con
/// su propio WasiState — aislamiento real entre modulos, no un contexto
/// compartido.
struct WasiState {
    ctx: WasiCtx,
    table: ResourceTable,
    http: WasiHttpCtx,
}

impl WasiState {
    fn new(env: &[(&str, String)]) -> Self {
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio();
        for (k, v) in env {
            builder.env(k, v);
        }
        Self {
            ctx: builder.build(),
            table: ResourceTable::new(),
            http: WasiHttpCtx::new(),
        }
    }
}

impl WasiView for WasiState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.ctx
    }
}

impl WasiHttpView for WasiState {
    fn ctx(&mut self) -> &mut WasiHttpCtx {
        &mut self.http
    }

    fn table(&mut self) -> &mut ResourceTable {
        &mut self.table
    }
}

// Cada bindgen! vive en su propio modulo: agent-module, tool-module y
// llm-module se compilan como mundos WIT separados aunque compartan
// package logico agentmodules:core@0.1.0, asi sus tipos generados
// (Message, CallError...) no colisionan entre si en el host.
mod agent_world {
    wasmtime::component::bindgen!({
        world: "agent-module",
        path: "../modules/agent-core/wit/world.wit",
    });
}

mod tool_world {
    wasmtime::component::bindgen!({
        world: "tool-module",
        path: "../modules/tool-echo/wit/world.wit",
    });
}

mod llm_world {
    wasmtime::component::bindgen!({
        world: "llm-module",
        path: "../modules/llm-call/wit/world.wit",
    });
}

/// Instancia de tool-echo en su propio Store, independiente del Store del
/// agente. Al vivir en Stores distintos, el host puede invocarla desde
/// dentro de los callbacks de import del agente sin pelear con el borrow
/// checker (serian dos objetos Store diferentes, no uno reentrante).
struct ToolInstance {
    store: Store<WasiState>,
    bindings: tool_world::ToolModule,
}

struct LlmInstance {
    store: Store<WasiState>,
    bindings: llm_world::LlmModule,
}

/// Estado del Store del agente: ademas de su propio WASI, mantiene
/// referencias a las instancias de tool/llm ya cargadas. Este es el punto
/// de extension para fase 2: en vez de un unico `tool` fijo, aqui iria un
/// registry con attach/detach dinamico resolviendo multiples modulos por
/// nombre/hash en runtime.
struct HostState {
    wasi: WasiState,
    tool: RefCell<ToolInstance>,
    llm: RefCell<LlmInstance>,
}

impl WasiView for HostState {
    fn table(&mut self) -> &mut ResourceTable {
        &mut self.wasi.table
    }

    fn ctx(&mut self) -> &mut WasiCtx {
        &mut self.wasi.ctx
    }
}

fn tool_err_to_agent(e: tool_world::agentmodules::core::types::CallError) -> agent_world::agentmodules::core::types::CallError {
    use agent_world::agentmodules::core::types::CallError as A;
    use tool_world::agentmodules::core::types::CallError as T;
    match e {
        T::InvalidInput(s) => A::InvalidInput(s),
        T::ExecutionFailed(s) => A::ExecutionFailed(s),
        T::NotFound(s) => A::NotFound(s),
    }
}

fn llm_err_to_agent(e: llm_world::agentmodules::core::types::CallError) -> agent_world::agentmodules::core::types::CallError {
    use agent_world::agentmodules::core::types::CallError as A;
    use llm_world::agentmodules::core::types::CallError as L;
    match e {
        L::InvalidInput(s) => A::InvalidInput(s),
        L::ExecutionFailed(s) => A::ExecutionFailed(s),
        L::NotFound(s) => A::NotFound(s),
    }
}

impl agent_world::agentmodules::core::types::Host for HostState {}

impl agent_world::agentmodules::core::tool::Host for HostState {
    fn describe(&mut self) -> String {
        let mut inst = self.tool.borrow_mut();
        let ToolInstance { store, bindings } = &mut *inst;
        bindings
            .agentmodules_core_tool()
            .call_describe(&mut *store)
            .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    fn invoke(
        &mut self,
        call: agent_world::agentmodules::core::types::ToolCall,
    ) -> Result<String, agent_world::agentmodules::core::types::CallError> {
        let mut inst = self.tool.borrow_mut();
        let ToolInstance { store, bindings } = &mut *inst;

        let tool_call = tool_world::agentmodules::core::types::ToolCall {
            name: call.name,
            args_json: call.args_json,
        };

        let result = bindings
            .agentmodules_core_tool()
            .call_invoke(&mut *store, &tool_call)
            .map_err(|e| agent_world::agentmodules::core::types::CallError::ExecutionFailed(e.to_string()))?;

        result.map_err(tool_err_to_agent)
    }
}

impl agent_world::agentmodules::core::llm::Host for HostState {
    fn complete(
        &mut self,
        messages: Vec<agent_world::agentmodules::core::types::Message>,
        model: String,
    ) -> Result<String, agent_world::agentmodules::core::types::CallError> {
        let mut inst = self.llm.borrow_mut();
        let LlmInstance { store, bindings } = &mut *inst;

        let msgs: Vec<llm_world::agentmodules::core::types::Message> = messages
            .into_iter()
            .map(|m| llm_world::agentmodules::core::types::Message {
                role: m.role,
                content: m.content,
            })
            .collect();

        let result = bindings
            .agentmodules_core_llm()
            .call_complete(&mut *store, &msgs, &model)
            .map_err(|e| agent_world::agentmodules::core::types::CallError::ExecutionFailed(e.to_string()))?;

        result.map_err(llm_err_to_agent)
    }
}

#[derive(Parser)]
#[command(about = "Host runtime para agentes/tools/llm empaquetados como componentes WASM")]
struct Args {
    /// Ruta al .wasm (componente) del agente a ejecutar.
    #[arg(long, default_value = "target/wasm32-wasip2/release/agent_core.wasm")]
    agent: String,

    /// Ruta al .wasm (componente) de la tool a linkear.
    #[arg(long, default_value = "target/wasm32-wasip2/release/tool_echo.wasm")]
    tool: String,

    /// Ruta al .wasm (componente) del llm-call a linkear.
    #[arg(long, default_value = "target/wasm32-wasip2/release/llm_call.wasm")]
    llm: String,

    /// Mensaje de usuario a mandar al agente.
    #[arg(long, default_value = "hola agente")]
    message: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let mut config = Config::new();
    config.wasm_component_model(true);
    let engine = Engine::new(&config)?;

    // --- instancia tool-echo en su propio Store ---
    let tool_component = Component::from_file(&engine, &args.tool)
        .map_err(|e| anyhow!("cargando tool '{}': {e}", args.tool))?;
    let mut tool_store = Store::new(&engine, WasiState::new(&[]));
    let mut tool_linker = Linker::<WasiState>::new(&engine);
    wasmtime_wasi::add_to_linker_sync(&mut tool_linker)?;
    let tool_bindings =
        tool_world::ToolModule::instantiate(&mut tool_store, &tool_component, &tool_linker)?;

    // --- instancia llm-call en su propio Store ---
    let llm_component = Component::from_file(&engine, &args.llm)
        .map_err(|e| anyhow!("cargando llm '{}': {e}", args.llm))?;
    let llm_env: Vec<(&str, String)> = std::env::var("OPENROUTER_API_KEY")
        .ok()
        .map(|k| ("OPENROUTER_API_KEY", k))
        .into_iter()
        .collect();
    let mut llm_store = Store::new(&engine, WasiState::new(&llm_env));
    let mut llm_linker = Linker::<WasiState>::new(&engine);
    wasmtime_wasi::add_to_linker_sync(&mut llm_linker)?;
    wasmtime_wasi_http::add_only_http_to_linker_sync(&mut llm_linker)?;
    let llm_bindings =
        llm_world::LlmModule::instantiate(&mut llm_store, &llm_component, &llm_linker)?;

    // --- instancia agent-core, linkeando sus imports (tool, llm) contra ---
    // --- las dos instancias de arriba via HostState ---
    let agent_component = Component::from_file(&engine, &args.agent)
        .map_err(|e| anyhow!("cargando agent '{}': {e}", args.agent))?;

    let state = HostState {
        wasi: WasiState::new(&[]),
        tool: RefCell::new(ToolInstance {
            store: tool_store,
            bindings: tool_bindings,
        }),
        llm: RefCell::new(LlmInstance {
            store: llm_store,
            bindings: llm_bindings,
        }),
    };

    let mut agent_store = Store::new(&engine, state);
    let mut agent_linker = Linker::<HostState>::new(&engine);
    wasmtime_wasi::add_to_linker_sync(&mut agent_linker)?;
    agent_world::AgentModule::add_to_linker(&mut agent_linker, |s| s)?;

    let agent_bindings =
        agent_world::AgentModule::instantiate(&mut agent_store, &agent_component, &agent_linker)?;

    let input = vec![agent_world::agentmodules::core::types::Message {
        role: "user".into(),
        content: args.message,
    }];

    let result = agent_bindings
        .agentmodules_core_agent()
        .call_run(&mut agent_store, &input)?;

    match result {
        Ok(output) => println!("{output}"),
        Err(e) => eprintln!("error del agente: {e:?}"),
    }

    Ok(())
}
