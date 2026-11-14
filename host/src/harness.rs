//! CLI harness: one router agent plus plugin agents. Each plugin is its own
//! WASM instance. The router reaches them through its ordinary `tool` import,
//! and so does any plugin granted `plugins`, which is how agents call each
//! other. What a plugin may touch is decided here, from the manifest.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use wasmtime::component::{HasSelf, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtxView, WasiView};

use crate::runtime::plugin_world::agentwos::core::workspace::{ExecResult, FileEntry, Host as WorkspaceHost};
use crate::runtime::plugin_world::PluginModule;
use crate::runtime::{
    load, message, AgentInstance, CallError, LlmClient, LlmHost, Message, ToolCall, ToolHost, Tools, TypesHost,
    WasiState,
};
use crate::workspace::Workspace;

const GRANTS: [&str; 3] = ["llm", "workspace", "plugins"];
const DEFAULT_TIMEOUT_SECONDS: u64 = 60;

/// One entry of `plugins` in the manifest.
struct PluginSpec {
    name: String,
    description: String,
    path: String,
    /// `llm`, `workspace`, `plugins`: the imports that work for this plugin.
    grants: Vec<String>,
    /// Programs it may run in the workspace.
    allow: Vec<String>,
    /// Project markers (`Cargo.toml`, `*.py`). With any, the plugin is only
    /// offered to other agents in a project that has one of them.
    when: Vec<String>,
    /// Its own model; the harness model otherwise.
    model: Option<String>,
}

impl PluginSpec {
    fn granted(&self, grant: &str) -> bool {
        self.grants.iter().any(|g| g == grant)
    }

    fn fits(&self, project: &Workspace) -> bool {
        self.when.is_empty() || self.when.iter().any(|marker| project.has(marker))
    }
}

/// `manifest.json` of a harness. Paths resolve from the manifest's folder.
struct Manifest {
    router: String,
    llm: String,
    model: Option<String>,
    /// Plugin that gets whatever is typed without a `/<plugin>` prefix.
    /// Without it, that text goes to the router's model, which picks plugins.
    default: Option<String>,
    workspace: PathBuf,
    timeout: Duration,
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
                allow: list(&p["allow"]),
                when: list(&p["when"]),
                model: p["model"].as_str().map(str::to_string),
                name,
                grants,
            });
        }
        if plugins.is_empty() {
            bail!("{}: no plugins", path.display());
        }
        let default = raw["default"].as_str().map(str::to_string);
        if let Some(name) = default.as_deref().filter(|d| plugins.iter().all(|p| p.name != *d)) {
            bail!("{}: `default` names '{name}', which is not a plugin", path.display());
        }

        Ok(Self {
            default,
            router: resolve(&text(&raw, "router")?),
            llm: resolve(&text(&raw, "llm")?),
            model: raw["model"].as_str().map(str::to_string),
            workspace: base.join(raw["workspace"].as_str().unwrap_or("workspace")),
            timeout: Duration::from_secs(raw["timeout_seconds"].as_u64().unwrap_or(DEFAULT_TIMEOUT_SECONDS)),
            plugins,
        })
    }
}

/// State of a plugin's Store. A capability the manifest did not grant is
/// simply absent, and the import answers with an error.
struct PluginState {
    wasi: WasiState,
    /// Used when the plugin asks for a completion without naming a model.
    model: String,
    llm: Option<LlmClient>,
    workspace: Option<Workspace>,
    plugins: Option<Plugins>,
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

/// The other plugins, for a plugin granted `plugins`.
impl ToolHost for PluginState {
    fn describe(&mut self) -> String {
        self.plugins.as_mut().map(Tools::describe).unwrap_or_else(|| json!({ "tools": [] }).to_string())
    }

    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        self.plugins.as_mut().ok_or_else(|| not_granted("plugins"))?.invoke(call)
    }
}

struct Plugin {
    spec: PluginSpec,
    /// Locked while the plugin runs, so a chain of calls cannot re-enter it.
    live: Mutex<(Store<PluginState>, PluginModule)>,
}

/// Every plugin of the harness. The router holds one of these as its tools,
/// and so does each plugin granted `plugins`.
#[derive(Clone)]
struct Plugins {
    all: Arc<OnceLock<Vec<Plugin>>>,
    project: Workspace,
}

impl Plugins {
    fn spawn(engine: &Engine, manifest: Manifest, project: &Workspace, model: &str) -> Result<Self> {
        let llm = load(engine, &manifest.llm)?;
        let mut linker = Linker::<PluginState>::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        PluginModule::add_to_linker::<_, HasSelf<PluginState>>(&mut linker, |s| s)?;

        let registry = Self { all: Arc::new(OnceLock::new()), project: project.clone() };
        let mut all = Vec::new();
        for spec in manifest.plugins {
            let state = PluginState {
                wasi: WasiState::new(&[]),
                model: spec.model.clone().unwrap_or_else(|| model.to_string()),
                llm: if spec.granted("llm") { Some(LlmClient::spawn(engine, &llm)?) } else { None },
                workspace: spec.granted("workspace").then(|| project.allowing(&spec.allow)),
                plugins: spec.granted("plugins").then(|| registry.clone()),
            };
            let mut store = Store::new(engine, state);
            let bindings = PluginModule::instantiate(&mut store, &load(engine, &spec.path)?, &linker)
                .map_err(|e| anyhow!("plugin '{}': {e}", spec.name))?;
            all.push(Plugin { spec, live: Mutex::new((store, bindings)) });
        }
        let _ = registry.all.set(all);
        Ok(registry)
    }

    fn all(&self) -> &[Plugin] {
        self.all.get().map(Vec::as_slice).unwrap_or_default()
    }
}

impl Tools for Plugins {
    /// The plugins that fit this project. The check is made on every call, so
    /// an expert appears as soon as the project gains what it works on.
    fn describe(&mut self) -> String {
        let fitting = self.all().iter().filter(|p| p.spec.fits(&self.project));
        let tools: Vec<_> =
            fitting.map(|p| json!({ "name": p.spec.name, "description": p.spec.description })).collect();
        json!({ "tools": tools }).to_string()
    }

    /// `call.name` picks the plugin; its input is the `input` argument.
    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        let plugin = self.all().iter().find(|p| p.spec.name == call.name).ok_or_else(|| {
            let names = self.all().iter().map(|p| p.spec.name.as_str()).collect::<Vec<_>>().join(", ");
            CallError::NotFound(format!("there is no plugin '{}' (plugins: {names})", call.name))
        })?;
        let Ok(mut live) = plugin.live.try_lock() else {
            return Err(CallError::InvalidInput(format!(
                "plugin '{}' is already running further up this chain of calls",
                call.name
            )));
        };
        let args: Value = serde_json::from_str(&call.args_json).unwrap_or(Value::Null);
        let input = args["input"].as_str().unwrap_or("");
        let (store, bindings) = &mut *live;
        bindings
            .agentwos_core_agent()
            .call_run(store, &[message("user", input)])
            .map_err(|e| CallError::ExecutionFailed(format!("plugin '{}' crashed: {e}", call.name)))?
    }
}

fn print_plugins(manifest: &Manifest, project: &Workspace) {
    let or_dash = |items: &[String], sep: &str| if items.is_empty() { "-".to_string() } else { items.join(sep) };
    for p in &manifest.plugins {
        let fits = if p.fits(project) { ' ' } else { 'x' };
        let about: String = p.description.chars().take_while(|c| !".:".contains(*c)).collect();
        println!("{fits} {:<9} {:<21} {:<9} {about}", p.name, or_dash(&p.grants, "+"), or_dash(&p.allow, ","));
    }
}

/// What the router receives. Text without a `/` goes to the default plugin
/// when the manifest names one. The router takes its model from a
/// `model:<id> ` prefix, like `agent-core`.
fn request(default: Option<&str>, model: &str, text: &str) -> Vec<Message> {
    let text = text.trim();
    match default {
        Some(plugin) if !text.starts_with('/') => vec![message("user", format!("model:{model} /{plugin} {text}"))],
        _ => vec![message("user", format!("model:{model} {text}"))],
    }
}

pub fn run(
    engine: &Engine,
    path: &Path,
    project: Option<PathBuf>,
    model: Option<&str>,
    text: Option<&str>,
    list: bool,
) -> Result<()> {
    let manifest = Manifest::load(path)?;
    let project = Workspace::new(project.unwrap_or_else(|| manifest.workspace.clone()), manifest.timeout);
    if list {
        print_plugins(&manifest, &project);
        return Ok(());
    }
    let model = model
        .map(str::to_string)
        .or_else(|| manifest.model.clone())
        .ok_or_else(|| anyhow!("no model: set `model` in the manifest or pass --model"))?;
    let default = manifest.default.clone();
    let (router, llm) = (load(engine, &manifest.router)?, load(engine, &manifest.llm)?);

    let interactive = text.is_none();
    if interactive {
        println!("AgentWOS harness, model {model}, project {}", project.root().display());
        println!("Plugins (x: not offered to agents in this project):");
        print_plugins(&manifest, &project);
        let rest = default.as_deref().map_or("the model".to_string(), |d| format!("`{d}`"));
        println!("\n/<plugin> <input> calls a plugin. Anything else goes to {rest}. /help, /exit.\n");
    }

    let plugins = Plugins::spawn(engine, manifest, &project, &model)?;
    let mut router = AgentInstance::new(engine, &router, &llm, Box::new(plugins))?;

    if let Some(text) = text {
        println!("{}", router.ask(request(default.as_deref(), &model, text)).map_err(|e| anyhow!(e))?);
        return Ok(());
    }

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
            match router.ask(request(default.as_deref(), &model, line)) {
                Ok(reply) => println!("{reply}\n"),
                Err(e) => println!("error: {e}\n"),
            }
        }
    }
    Ok(())
}
