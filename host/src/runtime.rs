use std::cell::RefCell;

use anyhow::{anyhow, Result};
use wasmtime::component::{Component, Linker, ResourceTable};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiView};
use wasmtime_wasi_http::{WasiHttpCtx, WasiHttpView};

// One bindgen! per world, each in its own module so the generated types
// (Message, CallError...) do not collide. All read the same canonical WIT.
pub mod agent_world {
    wasmtime::component::bindgen!({ world: "agent-module", path: "../wit/world.wit" });
}
pub mod tool_world {
    wasmtime::component::bindgen!({ world: "tool-module", path: "../wit/world.wit" });
}
pub mod llm_world {
    wasmtime::component::bindgen!({ world: "llm-module", path: "../wit/world.wit" });
}
pub mod control_world {
    wasmtime::component::bindgen!({ world: "control-module", path: "../wit/world.wit" });
}
pub mod scenario_world {
    wasmtime::component::bindgen!({ world: "scenario-module", path: "../wit/world.wit" });
}

use agent_world::agentwos::core::types as at;

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
        let load = |p: &str| Component::from_file(&engine, p).map_err(|e| anyhow!("loading '{p}': {e}"));
        Ok(Self {
            agent: load(agent)?,
            tool: load(tool)?,
            llm: load(llm)?,
            scenario: scenario.map(load).transpose()?,
            controls: controls
                .iter()
                .map(|p| Ok((module_name(p), load(p)?)))
                .collect::<Result<_>>()?,
            engine,
        })
    }
}

fn module_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

struct ToolInstance {
    store: Store<WasiState>,
    bindings: tool_world::ToolModule,
}

struct LlmInstance {
    store: Store<WasiState>,
    bindings: llm_world::LlmModule,
}

/// Agent store. Its imports (tool, llm) resolve against instances that live in
/// their own Stores: the host mediates every call between modules.
pub struct HostState {
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

fn trap(e: anyhow::Error) -> at::CallError {
    at::CallError::ExecutionFailed(e.to_string())
}

impl at::Host for HostState {}

impl agent_world::agentwos::core::tool::Host for HostState {
    fn describe(&mut self) -> String {
        let mut inst = self.tool.borrow_mut();
        let ToolInstance { store, bindings } = &mut *inst;
        bindings
            .agentwos_core_tool()
            .call_describe(store)
            .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"))
    }

    fn invoke(&mut self, call: at::ToolCall) -> Result<String, at::CallError> {
        use tool_world::agentwos::core::types as tt;
        let mut inst = self.tool.borrow_mut();
        let ToolInstance { store, bindings } = &mut *inst;
        let call = tt::ToolCall { name: call.name, args_json: call.args_json };
        bindings
            .agentwos_core_tool()
            .call_invoke(store, &call)
            .map_err(trap)?
            .map_err(|e| match e {
                tt::CallError::InvalidInput(s) => at::CallError::InvalidInput(s),
                tt::CallError::ExecutionFailed(s) => at::CallError::ExecutionFailed(s),
                tt::CallError::NotFound(s) => at::CallError::NotFound(s),
            })
    }
}

impl agent_world::agentwos::core::llm::Host for HostState {
    fn complete(&mut self, messages: Vec<at::Message>, model: String) -> Result<String, at::CallError> {
        use llm_world::agentwos::core::types as lt;
        let mut inst = self.llm.borrow_mut();
        let LlmInstance { store, bindings } = &mut *inst;
        let msgs: Vec<lt::Message> = messages
            .into_iter()
            .map(|m| lt::Message { role: m.role, content: m.content })
            .collect();
        bindings
            .agentwos_core_llm()
            .call_complete(store, &msgs, &model)
            .map_err(trap)?
            .map_err(|e| match e {
                lt::CallError::InvalidInput(s) => at::CallError::InvalidInput(s),
                lt::CallError::ExecutionFailed(s) => at::CallError::ExecutionFailed(s),
                lt::CallError::NotFound(s) => at::CallError::NotFound(s),
            })
    }
}

/// Live agent instance, with its private tool and llm.
pub struct AgentInstance {
    store: Store<HostState>,
    bindings: agent_world::AgentModule,
}

impl AgentInstance {
    pub fn spawn(c: &Components) -> Result<Self> {
        let mut tool_store = Store::new(&c.engine, WasiState::new(&[]));
        let mut tool_linker = Linker::<WasiState>::new(&c.engine);
        wasmtime_wasi::add_to_linker_sync(&mut tool_linker)?;
        let tool = tool_world::ToolModule::instantiate(&mut tool_store, &c.tool, &tool_linker)?;

        // Only the llm receives network access and the API key.
        let llm_env: Vec<(&str, String)> = std::env::var("OPENROUTER_API_KEY")
            .ok()
            .map(|k| ("OPENROUTER_API_KEY", k))
            .into_iter()
            .collect();
        let mut llm_store = Store::new(&c.engine, WasiState::new(&llm_env));
        let mut llm_linker = Linker::<WasiState>::new(&c.engine);
        wasmtime_wasi::add_to_linker_sync(&mut llm_linker)?;
        wasmtime_wasi_http::add_only_http_to_linker_sync(&mut llm_linker)?;
        let llm = llm_world::LlmModule::instantiate(&mut llm_store, &c.llm, &llm_linker)?;

        let state = HostState {
            wasi: WasiState::new(&[]),
            tool: RefCell::new(ToolInstance { store: tool_store, bindings: tool }),
            llm: RefCell::new(LlmInstance { store: llm_store, bindings: llm }),
        };
        let mut store = Store::new(&c.engine, state);
        let mut linker = Linker::<HostState>::new(&c.engine);
        wasmtime_wasi::add_to_linker_sync(&mut linker)?;
        agent_world::AgentModule::add_to_linker(&mut linker, |s| s)?;
        let bindings = agent_world::AgentModule::instantiate(&mut store, &c.agent, &linker)?;
        Ok(Self { store, bindings })
    }

    pub fn ask(&mut self, messages: Vec<at::Message>) -> Result<String, String> {
        let input = messages;
        match self.bindings.agentwos_core_agent().call_run(&mut self.store, &input) {
            Ok(Ok(s)) => Ok(s),
            Ok(Err(e)) => Err(format!("error del agente: {e:?}")),
            Err(e) => Err(format!("trap del agente: {e}")),
        }
    }
}

/// Live scenario instance: owns the world and the record of what happened.
pub fn agent_message(role: String, content: String) -> at::Message {
    at::Message { role, content }
}

pub struct ScenarioInstance {
    store: Store<WasiState>,
    bindings: scenario_world::ScenarioModule,
}

type Msg = scenario_world::agentwos::core::types::Message;

impl ScenarioInstance {
    pub fn spawn(c: &Components, component: &Component) -> Result<Self> {
        let mut store = Store::new(&c.engine, WasiState::new(&[]));
        let mut linker = Linker::<WasiState>::new(&c.engine);
        wasmtime_wasi::add_to_linker_sync(&mut linker)?;
        let bindings = scenario_world::ScenarioModule::instantiate(&mut store, component, &linker)?;
        Ok(Self { store, bindings })
    }

    pub fn setup(&mut self, config: &str) -> Result<String> {
        let api = self.bindings.agentwos_core_scenario();
        api.call_setup(&mut self.store, config)
    }

    pub fn begin_day(&mut self, day: u32, total: u32) -> Result<String> {
        let api = self.bindings.agentwos_core_scenario();
        api.call_begin_day(&mut self.store, day, total)
    }

    pub fn turn_messages(&mut self, agent: u32) -> Result<Vec<(String, String)>> {
        let api = self.bindings.agentwos_core_scenario();
        let msgs: Vec<Msg> = api.call_turn_messages(&mut self.store, agent)?;
        Ok(msgs.into_iter().map(|m| (m.role, m.content)).collect())
    }

    pub fn action_of(&mut self, agent: u32, reply: &str) -> Result<String> {
        let api = self.bindings.agentwos_core_scenario();
        api.call_action_of(&mut self.store, agent, reply)
    }

    pub fn context(&mut self) -> Result<String> {
        let api = self.bindings.agentwos_core_scenario();
        api.call_context(&mut self.store)
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
        api.call_record_turn(&mut self.store, agent, delivered, controls, latency_ms, error)
    }

    pub fn end_day(&mut self) -> Result<bool> {
        let api = self.bindings.agentwos_core_scenario();
        api.call_end_day(&mut self.store)
    }

    pub fn snapshot(&mut self) -> Result<String> {
        let api = self.bindings.agentwos_core_scenario();
        api.call_snapshot(&mut self.store)
    }
}
