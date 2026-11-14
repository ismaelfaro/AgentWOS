//! Running components: WASI state, the generated bindings, and live instances
//! of agents, tools, the LLM client and scenarios. Every instance has its own
//! Store, and the host mediates each call between them.

use anyhow::{anyhow, Result};
use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpCtxView, WasiHttpView};

// One bindgen! per world. `agent_world` generates the shared interfaces
// (`types`, `llm`, `tool`); the others reuse them through `with`, so a
// `CallError` or `Message` is the same Rust type everywhere in the host.
pub mod agent_world {
    wasmtime::component::bindgen!({ world: "agent-module", path: "../wit/world.wit" });
}
pub mod tool_world {
    wasmtime::component::bindgen!({
        world: "tool-module", path: "../wit/world.wit",
        with: { "agentwos:core/types": crate::runtime::agent_world::agentwos::core::types },
    });
}
pub mod llm_world {
    wasmtime::component::bindgen!({
        world: "llm-module", path: "../wit/world.wit",
        with: { "agentwos:core/types": crate::runtime::agent_world::agentwos::core::types },
    });
}
pub mod scenario_world {
    wasmtime::component::bindgen!({
        world: "scenario-module", path: "../wit/world.wit",
        with: { "agentwos:core/types": crate::runtime::agent_world::agentwos::core::types },
    });
}
pub mod plugin_world {
    wasmtime::component::bindgen!({
        world: "plugin-module", path: "../wit/world.wit",
        with: {
            "agentwos:core/types": crate::runtime::agent_world::agentwos::core::types,
            "agentwos:core/llm": crate::runtime::agent_world::agentwos::core::llm,
        },
    });
}
pub mod control_world {
    wasmtime::component::bindgen!({ world: "control-module", path: "../wit/world.wit" });
}

pub use agent_world::agentwos::core::llm::Host as LlmHost;
pub use agent_world::agentwos::core::types::{CallError, Host as TypesHost, Message, ToolCall};

/// The message inside a `CallError`, for people.
pub fn error_text(e: &CallError) -> &str {
    match e {
        CallError::InvalidInput(s) | CallError::ExecutionFailed(s) | CallError::NotFound(s) => s,
    }
}

/// A trap is a crash of the component, not an error it reported.
fn trap(e: wasmtime::Error) -> CallError {
    CallError::ExecutionFailed(format!("component crashed: {e}"))
}

pub fn message(role: &str, content: impl Into<String>) -> Message {
    Message { role: role.to_string(), content: content.into() }
}

/// WASI context of one Store. Each component has its own: isolation is per
/// instance, and capabilities (env, network) are granted one by one.
pub struct WasiState {
    ctx: WasiCtx,
    table: ResourceTable,
    http: WasiHttpCtx,
}

impl WasiState {
    pub fn new(env: &[(&str, String)]) -> Self {
        let mut builder = WasiCtxBuilder::new();
        builder.inherit_stdio();
        for (k, v) in env {
            builder.env(k, v);
        }
        Self { ctx: builder.build(), table: ResourceTable::new(), http: WasiHttpCtx::new() }
    }
}

impl WasiView for WasiState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.ctx, table: &mut self.table }
    }
}

impl WasiHttpView for WasiState {
    fn http(&mut self) -> WasiHttpCtxView<'_> {
        WasiHttpCtxView {
            ctx: &mut self.http,
            table: &mut self.table,
            hooks: wasmtime_wasi_http::default_hooks(),
        }
    }
}

/// A Store and linker for a component that needs nothing beyond plain WASI.
pub fn plain_store(engine: &Engine) -> Result<(Store<WasiState>, Linker<WasiState>)> {
    let mut linker = Linker::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    Ok((Store::new(engine, WasiState::new(&[])), linker))
}

pub fn load(engine: &Engine, path: &str) -> Result<Component> {
    Component::from_file(engine, path).map_err(|e| anyhow!("loading '{path}': {e}"))
}

/// Components compiled once; instantiated as many times as needed.
pub struct Components {
    pub engine: Engine,
    pub agent: Component,
    pub tool: Component,
    pub llm: Component,
    pub scenario: Option<Component>,
    pub controls: Vec<(String, Component)>,
}

impl Components {
    pub fn load(
        engine: Engine,
        agent: &str,
        tool: &str,
        llm: &str,
        scenario: Option<&str>,
        controls: &[String],
    ) -> Result<Self> {
        let name = |path: &str| {
            std::path::Path::new(path).file_stem().map_or(path.to_string(), |s| s.to_string_lossy().into_owned())
        };
        Ok(Self {
            agent: load(&engine, agent)?,
            tool: load(&engine, tool)?,
            llm: load(&engine, llm)?,
            scenario: scenario.map(|p| load(&engine, p)).transpose()?,
            controls: controls.iter().map(|p| Ok((name(p), load(&engine, p)?))).collect::<Result<_>>()?,
            engine,
        })
    }
}

/// What an agent's `tool` import resolves to: one tool component, or anything
/// else that can describe itself and take calls (the harness plugin registry).
pub trait Tools: Send {
    fn describe(&mut self) -> String;
    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError>;
}

/// A live tool component.
pub struct ToolInstance {
    store: Store<WasiState>,
    bindings: tool_world::ToolModule,
}

impl ToolInstance {
    pub fn spawn(engine: &Engine, component: &Component) -> Result<Self> {
        let (mut store, linker) = plain_store(engine)?;
        let bindings = tool_world::ToolModule::instantiate(&mut store, component, &linker)?;
        Ok(Self { store, bindings })
    }
}

impl Tools for ToolInstance {
    fn describe(&mut self) -> String {
        self.bindings
            .agentwos_core_tool()
            .call_describe(&mut self.store)
            .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        self.bindings.agentwos_core_tool().call_invoke(&mut self.store, &call).map_err(trap)?
    }
}

/// A live LLM client. It is the only component that gets network access and
/// the API key, so whoever holds one of these is who can reach a model.
pub struct LlmClient {
    store: Store<WasiState>,
    bindings: llm_world::LlmModule,
}

impl LlmClient {
    pub fn spawn(engine: &Engine, component: &Component) -> Result<Self> {
        let env: Vec<(&str, String)> =
            std::env::var("OPENROUTER_API_KEY").ok().map(|k| ("OPENROUTER_API_KEY", k)).into_iter().collect();
        let mut store = Store::new(engine, WasiState::new(&env));
        let mut linker = Linker::<WasiState>::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        wasmtime_wasi_http::p2::add_only_http_to_linker_sync(&mut linker)?;
        let bindings = llm_world::LlmModule::instantiate(&mut store, component, &linker)?;
        Ok(Self { store, bindings })
    }

    pub fn complete(&mut self, messages: &[Message], model: &str) -> Result<String, CallError> {
        self.bindings.agentwos_core_llm().call_complete(&mut self.store, messages, model).map_err(trap)?
    }
}

/// State of an agent's Store: its imports resolve against instances that live
/// in their own Stores.
pub struct AgentState {
    wasi: WasiState,
    tools: Box<dyn Tools>,
    llm: LlmClient,
}

impl WasiView for AgentState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        self.wasi.ctx()
    }
}

impl TypesHost for AgentState {}

impl agent_world::agentwos::core::tool::Host for AgentState {
    fn describe(&mut self) -> String {
        self.tools.describe()
    }

    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        self.tools.invoke(call)
    }
}

impl LlmHost for AgentState {
    fn complete(&mut self, messages: Vec<Message>, model: String) -> Result<String, CallError> {
        self.llm.complete(&messages, &model)
    }
}

/// A live agent, with its own LLM client and whatever answers its tool calls.
pub struct AgentInstance {
    store: Store<AgentState>,
    bindings: agent_world::AgentModule,
}

impl AgentInstance {
    pub fn new(engine: &Engine, agent: &Component, llm: &Component, tools: Box<dyn Tools>) -> Result<Self> {
        let state = AgentState { wasi: WasiState::new(&[]), tools, llm: LlmClient::spawn(engine, llm)? };
        let mut store = Store::new(engine, state);
        let mut linker = Linker::<AgentState>::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        agent_world::AgentModule::add_to_linker::<_, HasSelf<AgentState>>(&mut linker, |s| s)?;
        let bindings = agent_world::AgentModule::instantiate(&mut store, agent, &linker)?;
        Ok(Self { store, bindings })
    }

    /// An agent wired to the single tool component in `c`.
    pub fn spawn(c: &Components) -> Result<Self> {
        Self::new(&c.engine, &c.agent, &c.llm, Box::new(ToolInstance::spawn(&c.engine, &c.tool)?))
    }

    pub fn ask(&mut self, messages: Vec<Message>) -> Result<String, String> {
        match self.bindings.agentwos_core_agent().call_run(&mut self.store, &messages) {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(e)) => Err(error_text(&e).to_string()),
            Err(e) => Err(format!("agent crashed: {e}")),
        }
    }
}

/// Live scenario instance: owns the world and the record of what happened.
pub struct ScenarioInstance {
    store: Store<WasiState>,
    bindings: scenario_world::ScenarioModule,
}

impl ScenarioInstance {
    pub fn spawn(c: &Components, component: &Component) -> Result<Self> {
        let (mut store, linker) = plain_store(&c.engine)?;
        let bindings = scenario_world::ScenarioModule::instantiate(&mut store, component, &linker)?;
        Ok(Self { store, bindings })
    }

    pub fn setup(&mut self, config: &str) -> Result<String> {
        Ok(self.bindings.agentwos_core_scenario().call_setup(&mut self.store, config)?)
    }

    pub fn begin_day(&mut self, day: u32, total: u32) -> Result<String> {
        Ok(self.bindings.agentwos_core_scenario().call_begin_day(&mut self.store, day, total)?)
    }

    pub fn turn_messages(&mut self, agent: u32) -> Result<Vec<Message>> {
        Ok(self.bindings.agentwos_core_scenario().call_turn_messages(&mut self.store, agent)?)
    }

    pub fn action_of(&mut self, agent: u32, reply: &str) -> Result<String> {
        Ok(self.bindings.agentwos_core_scenario().call_action_of(&mut self.store, agent, reply)?)
    }

    pub fn context(&mut self) -> Result<String> {
        Ok(self.bindings.agentwos_core_scenario().call_context(&mut self.store)?)
    }

    pub fn record_turn(
        &mut self,
        agent: u32,
        delivered: Option<&str>,
        controls: &str,
        latency_ms: u64,
        error: Option<&str>,
    ) -> Result<()> {
        let api = self.bindings.agentwos_core_scenario();
        Ok(api.call_record_turn(&mut self.store, agent, delivered, controls, latency_ms, error)?)
    }

    pub fn end_day(&mut self) -> Result<bool> {
        Ok(self.bindings.agentwos_core_scenario().call_end_day(&mut self.store)?)
    }

    pub fn snapshot(&mut self) -> Result<String> {
        Ok(self.bindings.agentwos_core_scenario().call_snapshot(&mut self.store)?)
    }
}
