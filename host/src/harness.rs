//! CLI harness: one router agent plus plugin agents. Each plugin is its own
//! WASM instance, and the router reaches all of them through its ordinary
//! `tool` import. What a plugin may touch is decided here, from the manifest.

use std::io::{self, BufRead, Write};
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use wasmtime::component::{HasSelf, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtxView, WasiView};

use crate::runtime::plugin_world::agentwos::core::workspace::{ExecResult, FileEntry, Host as WorkspaceHost};
use crate::runtime::plugin_world::PluginModule;
use crate::runtime::{
    load, message, AgentInstance, CallError, LlmClient, LlmHost, Message, ToolCall, Tools, TypesHost, WasiState,
};
use crate::workspace::Workspace;

const GRANTS: [&str; 2] = ["llm", "workspace"];
const DEFAULT_TIMEOUT_SECONDS: u64 = 30;

struct PluginSpec {
    name: String,
    description: String,
    path: String,
    grants: Vec<String>,
}

/// `manifest.json` of a harness. Paths resolve from the manifest's folder.
struct Manifest {
    router: String,
    llm: String,
    model: Option<String>,
    workspace: Workspace,
    plugins: Vec<PluginSpec>,
}

impl Manifest {
    fn load(path: &Path) -> Result<Self> {
        let raw: Value = serde_json::from_str(&std::fs::read_to_string(path)?)
            .map_err(|e| anyhow!("{}: {e}", path.display()))?;
        let base = path.parent().unwrap_or(Path::new("."));
        let resolve = |p: &str| base.join(p).to_string_lossy().into_owned();
        let text = |value: &Value, key: &str| {
            value[key].as_str().map(str::to_string).ok_or_else(|| anyhow!("{}: missing `{key}`", path.display()))
        };
        let list = |value: &Value| -> Vec<String> {
            value.as_array().into_iter().flatten().filter_map(Value::as_str).map(str::to_string).collect()
        };

        let mut plugins = Vec::new();
        for p in raw["plugins"].as_array().into_iter().flatten() {
            let name = text(p, "name")?;
            let grants = list(&p["grants"]);
            if let Some(unknown) = grants.iter().find(|g| !GRANTS.contains(&g.as_str())) {
                bail!("plugin '{name}': unknown grant '{unknown}' (known: {})", GRANTS.join(", "));
            }
            plugins.push(PluginSpec {
                description: p["description"].as_str().unwrap_or("").to_string(),
                path: resolve(&text(p, "agent")?),
                name,
                grants,
            });
        }
        if plugins.is_empty() {
            bail!("{}: no plugins", path.display());
        }

        Ok(Self {
            router: resolve(&text(&raw, "router")?),
            llm: resolve(&text(&raw, "llm")?),
            model: raw["model"].as_str().map(str::to_string),
            workspace: Workspace::new(
                base.join(raw["workspace"].as_str().unwrap_or("workspace")),
                list(&raw["allow"]),
                Duration::from_secs(raw["timeout_seconds"].as_u64().unwrap_or(DEFAULT_TIMEOUT_SECONDS)),
            ),
            plugins,
        })
    }
}

/// State of a plugin's Store. A capability the manifest did not grant is
/// simply absent, and the import answers with an error.
struct PluginState {
    wasi: WasiState,
    /// The harness model, used when a plugin asks for a completion without naming one.
    model: String,
    llm: Option<LlmClient>,
    workspace: Option<Workspace>,
}

fn not_granted(what: &str) -> CallError {
    CallError::InvalidInput(format!("this plugin was not granted `{what}` in the manifest"))
}

impl PluginState {
    fn workspace(&self) -> Result<&Workspace, CallError> {
        self.workspace.as_ref().ok_or_else(|| not_granted("workspace"))
    }
}

impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        self.wasi.ctx()
    }
}

impl TypesHost for PluginState {}

impl LlmHost for PluginState {
    fn complete(&mut self, messages: Vec<Message>, model: String) -> Result<String, CallError> {
        let model = if model.is_empty() { &self.model } else { &model };
        self.llm.as_mut().ok_or_else(|| not_granted("llm"))?.complete(&messages, model)
    }
}

impl WorkspaceHost for PluginState {
    fn read(&mut self, path: String) -> Result<String, CallError> {
        self.workspace()?.read(&path)
    }

    fn write(&mut self, path: String, contents: String) -> Result<String, CallError> {
        self.workspace()?.write(&path, &contents)
    }

    fn list_files(&mut self, path: String) -> Result<Vec<FileEntry>, CallError> {
        let entries = self.workspace()?.list(&path)?;
        Ok(entries.into_iter().map(|e| FileEntry { path: e.path, kind: e.kind.into(), bytes: e.bytes }).collect())
    }

    fn exec(&mut self, program: String, args: Vec<String>) -> Result<ExecResult, CallError> {
        let run = self.workspace()?.exec(&program, &args)?;
        Ok(ExecResult { status: run.status, stdout: run.stdout, stderr: run.stderr })
    }
}

struct Plugin {
    name: String,
    description: String,
    store: Store<PluginState>,
    bindings: PluginModule,
}

/// Every plugin of the harness, presented to the router as its tools.
struct Plugins(Vec<Plugin>);

impl Plugins {
    fn spawn(engine: &Engine, manifest: &Manifest, model: &str) -> Result<Self> {
        let llm = load(engine, &manifest.llm)?;
        let mut linker = Linker::<PluginState>::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        PluginModule::add_to_linker::<_, HasSelf<PluginState>>(&mut linker, |s| s)?;

        let mut plugins = Vec::new();
        for spec in &manifest.plugins {
            let granted = |grant: &str| spec.grants.iter().any(|g| g == grant);
            let state = PluginState {
                wasi: WasiState::new(&[]),
                model: model.to_string(),
                llm: if granted("llm") { Some(LlmClient::spawn(engine, &llm)?) } else { None },
                workspace: granted("workspace").then(|| manifest.workspace.clone()),
            };
            let mut store = Store::new(engine, state);
            let bindings = PluginModule::instantiate(&mut store, &load(engine, &spec.path)?, &linker)
                .map_err(|e| anyhow!("plugin '{}': {e}", spec.name))?;
            plugins.push(Plugin {
                name: spec.name.clone(),
                description: spec.description.clone(),
                store,
                bindings,
            });
        }
        Ok(Self(plugins))
    }
}

impl Tools for Plugins {
    fn describe(&mut self) -> String {
        let tools: Vec<_> = self.0.iter().map(|p| json!({ "name": p.name, "description": p.description })).collect();
        json!({ "tools": tools }).to_string()
    }

    /// `call.name` picks the plugin; its input is the `input` argument.
    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        let names = self.0.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
        let plugin = self.0.iter_mut().find(|p| p.name == call.name).ok_or_else(|| {
            CallError::NotFound(format!("there is no plugin '{}' (plugins: {names})", call.name))
        })?;
        let args: Value = serde_json::from_str(&call.args_json).unwrap_or(Value::Null);
        let input = args["input"].as_str().unwrap_or("");
        plugin
            .bindings
            .agentwos_core_agent()
            .call_run(&mut plugin.store, &[message("user", input)])
            .map_err(|e| CallError::ExecutionFailed(format!("plugin '{}' crashed: {e}", call.name)))?
    }
}

fn print_plugins(manifest: &Manifest) {
    for p in &manifest.plugins {
        let grants = if p.grants.is_empty() { "-".to_string() } else { p.grants.join("+") };
        println!("{:<11} {:<14} {}", p.name, grants, p.description);
    }
}

/// The router takes its model from a `model:<id> ` prefix, like `agent-core`.
fn with_model(model: &str, text: &str) -> Vec<Message> {
    vec![message("user", format!("model:{model} {text}"))]
}

pub fn run(engine: &Engine, path: &Path, model: Option<&str>, text: Option<&str>, list: bool) -> Result<()> {
    let manifest = Manifest::load(path)?;
    if list {
        print_plugins(&manifest);
        return Ok(());
    }
    let model = model
        .map(str::to_string)
        .or_else(|| manifest.model.clone())
        .ok_or_else(|| anyhow!("no model: set `model` in the manifest or pass --model"))?;

    let plugins = Plugins::spawn(engine, &manifest, &model)?;
    let mut router = AgentInstance::new(
        engine,
        &load(engine, &manifest.router)?,
        &load(engine, &manifest.llm)?,
        Box::new(plugins),
    )?;

    if let Some(text) = text {
        println!("{}", router.ask(with_model(&model, text)).map_err(|e| anyhow!(e))?);
        return Ok(());
    }

    println!("AgentWOS harness, model {model}. Plugins:");
    print_plugins(&manifest);
    println!("\n/<plugin> <input> calls a plugin. Anything else goes to the model. /help, /exit.\n");
    let mut lines = io::stdin().lock().lines();
    loop {
        print!("> ");
        io::stdout().flush()?;
        let Some(line) = lines.next().transpose()? else { break };
        let line = line.trim();
        if line == "/exit" || line == "/quit" {
            break;
        }
        if !line.is_empty() {
            match router.ask(with_model(&model, line)) {
                Ok(reply) => println!("{reply}\n"),
                Err(e) => println!("error: {e}\n"),
            }
        }
    }
    Ok(())
}
