//! CLI harness: one router agent plus plugin agents. Each plugin is its own
//! WASM instance. The router reaches them through its ordinary `tool` import,
//! and so does any plugin granted `plugins`, which is how agents call each
//! other. What a plugin may touch is decided here, from the manifest.

use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Result};
use serde_json::{json, Value};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtxView, WasiView};

use crate::runtime::plugin_world::agentwos::core::workspace::{ExecResult, FileEntry, Host as WorkspaceHost};
use crate::runtime::plugin_world::agentwos::core::team::Host as TeamHost;
use crate::runtime::plugin_world::PluginModule;
use crate::runtime::{
    load, message, AgentInstance, CallError, LlmClient, LlmHost, Message, ToolCall, ToolHost, Tools, TypesHost,
    WasiState,
};
use crate::activity::Activity;
use crate::workspace::Workspace;

const GRANTS: [&str; 3] = ["llm", "workspace", "plugins"];
const DEFAULT_TIMEOUT_SECONDS: u64 = 60;
/// How many agents deep a chain of calls may go.
const MAX_DEPTH: usize = 4;

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
    /// The other plugins, at this instance's place in the chain of calls.
    plugins: Option<Plugins>,
    activity: Activity,
}

fn not_granted(what: &str) -> CallError {
    CallError::InvalidInput(format!("this plugin was not granted `{what}` in the manifest"))
}

impl PluginState {
    fn workspace(&self) -> Result<&Workspace, CallError> {
        self.workspace.as_ref().ok_or_else(|| not_granted("workspace"))
    }

    fn plugins(&mut self) -> Result<&mut Plugins, CallError> {
        self.plugins.as_mut().ok_or_else(|| not_granted("plugins"))
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
        let llm = self.llm.as_mut().ok_or_else(|| not_granted("llm"))?;
        self.activity.thinking(model, || llm.complete(&messages, model))
    }
}

impl WorkspaceHost for PluginState {
    fn read(&mut self, path: String) -> Result<String, CallError> {
        let result = self.workspace()?.read(&path);
        self.activity.did(&format!("read {path}"), &result, |text| format!("{} lines", text.lines().count()));
        result
    }

    fn write(&mut self, path: String, contents: String) -> Result<String, CallError> {
        let result = self.workspace()?.write(&path, &contents);
        self.activity.did(&format!("write {path}"), &result, |_| format!("{} lines", contents.lines().count()));
        result
    }

    fn list_files(&mut self, path: String) -> Result<Vec<FileEntry>, CallError> {
        let entries = self.workspace()?.list(&path)?;
        Ok(entries.into_iter().map(|e| FileEntry { path: e.path, kind: e.kind.into(), bytes: e.bytes }).collect())
    }

    fn exec(&mut self, program: String, args: Vec<String>) -> Result<ExecResult, CallError> {
        let workspace = self.workspace()?;
        let command = format!("{program} {}", args.join(" "));
        let run = self.activity.program(
            &command,
            || workspace.exec(&program, &args),
            |run| (run.status, &run.stdout, &run.stderr),
        )?;
        Ok(ExecResult { status: run.status, stdout: run.stdout, stderr: run.stderr })
    }
}

/// The other plugins, for a plugin granted `plugins`.
impl ToolHost for PluginState {
    fn describe(&mut self) -> String {
        self.plugins.as_mut().map(Tools::describe).unwrap_or_else(|| json!({ "tools": [] }).to_string())
    }

    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        self.plugins()?.invoke(call)
    }
}

impl TeamHost for PluginState {
    fn invoke_all(&mut self, calls: Vec<ToolCall>) -> Vec<Result<String, CallError>> {
        match self.plugins() {
            Ok(plugins) => plugins.invoke_all(calls),
            Err(e) => calls.iter().map(|_| Err(e.clone())).collect(),
        }
    }
}

type Instance = (Store<PluginState>, PluginModule);

struct Plugin {
    spec: PluginSpec,
    component: Component,
    /// Instances not running right now. A call takes one, or makes a new one
    /// when all are busy, so the same agent can work on several things at once.
    idle: Mutex<Vec<Instance>>,
}

/// What every plugin instance is made from.
struct Registry {
    engine: Engine,
    linker: Linker<PluginState>,
    llm: Component,
    model: String,
    project: Workspace,
    plugins: Vec<Plugin>,
}

/// The plugins of the harness, seen from one place in a chain of calls. The
/// router holds one of these as its tools, and so does each plugin granted
/// `plugins`.
#[derive(Clone)]
struct Plugins {
    registry: Arc<Registry>,
    /// How many agents deep this holder is.
    depth: usize,
    activity: Activity,
}

impl Plugins {
    fn load(engine: &Engine, manifest: Manifest, project: &Workspace, model: &str, activity: &Activity) -> Result<Self> {
        let mut linker = Linker::<PluginState>::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        PluginModule::add_to_linker::<_, HasSelf<PluginState>>(&mut linker, |s| s)?;

        let mut plugins = Vec::new();
        for spec in manifest.plugins {
            let component = load(engine, &spec.path)?;
            plugins.push(Plugin { spec, component, idle: Mutex::new(Vec::new()) });
        }
        let registry = Arc::new(Registry {
            engine: engine.clone(),
            linker,
            llm: load(engine, &manifest.llm)?,
            model: model.to_string(),
            project: project.clone(),
            plugins,
        });
        // One instance of each now, so a plugin that cannot start says so here.
        let all = Self { registry, depth: 0, activity: activity.clone() };
        for plugin in &all.registry.plugins {
            let instance = all.instantiate(plugin).map_err(|e| anyhow!("plugin '{}': {e}", plugin.spec.name))?;
            plugin.idle.lock().unwrap().push(instance);
        }
        Ok(all)
    }

    fn instantiate(&self, plugin: &Plugin) -> Result<Instance> {
        let (registry, spec) = (&self.registry, &plugin.spec);
        let state = PluginState {
            wasi: WasiState::new(&[]),
            model: spec.model.clone().unwrap_or_else(|| registry.model.clone()),
            llm: if spec.granted("llm") { Some(LlmClient::spawn(&registry.engine, &registry.llm)?) } else { None },
            workspace: spec.granted("workspace").then(|| registry.project.allowing(&spec.allow)),
            plugins: spec.granted("plugins").then(|| self.clone()),
            activity: self.activity.clone(),
        };
        let mut store = Store::new(&registry.engine, state);
        let bindings = PluginModule::instantiate(&mut store, &plugin.component, &registry.linker)?;
        Ok((store, bindings))
    }

    /// Asks several plugins at the same time, each on its own thread and its
    /// own instance. Answers come back in the order of the calls.
    fn invoke_all(&mut self, calls: Vec<ToolCall>) -> Vec<Result<String, CallError>> {
        if calls.len() < 2 {
            return calls.into_iter().map(|call| self.invoke(call)).collect();
        }
        let input = |call: &ToolCall| -> String {
            let args: Value = serde_json::from_str(&call.args_json).unwrap_or(Value::Null);
            args["input"].as_str().unwrap_or("").to_string()
        };
        let asks: Vec<(String, String)> = calls.iter().map(|call| (call.name.clone(), input(call))).collect();
        self.activity.together(&asks);
        let answers = std::thread::scope(|scope| {
            let running: Vec<_> = calls
                .into_iter()
                .enumerate()
                .map(|(i, call)| {
                    let mut branch = Self { activity: self.activity.branch(i + 1), ..self.clone() };
                    scope.spawn(move || branch.invoke(call))
                })
                .collect();
            let crashed = || Err(CallError::ExecutionFailed("the agent's thread crashed".into()));
            running.into_iter().map(|thread| thread.join().unwrap_or_else(|_| crashed())).collect()
        });
        self.activity.settled();
        answers
    }
}

impl Tools for Plugins {
    /// The plugins that fit this project. The check is made on every call, so
    /// an expert appears as soon as the project gains what it works on.
    fn describe(&mut self) -> String {
        let fitting = self.registry.plugins.iter().filter(|p| p.spec.fits(&self.registry.project));
        let tools: Vec<_> = fitting
            .map(|p| json!({ "name": p.spec.name, "description": p.spec.description, "when": p.spec.when }))
            .collect();
        json!({ "tools": tools }).to_string()
    }

    /// `call.name` picks the plugin; its input is the `input` argument.
    fn invoke(&mut self, call: ToolCall) -> Result<String, CallError> {
        let plugins = &self.registry.plugins;
        let plugin = plugins.iter().find(|p| p.spec.name == call.name).ok_or_else(|| {
            let names = plugins.iter().map(|p| p.spec.name.as_str()).collect::<Vec<_>>().join(", ");
            CallError::NotFound(format!("there is no plugin '{}' (plugins: {names})", call.name))
        })?;
        if self.depth >= MAX_DEPTH {
            return Err(CallError::InvalidInput(format!(
                "'{}' was not asked: agents are already {MAX_DEPTH} deep in asking each other",
                call.name
            )));
        }
        let args: Value = serde_json::from_str(&call.args_json).unwrap_or(Value::Null);
        let input = args["input"].as_str().unwrap_or("");

        // What the plugin does, and whoever it asks, is one level further in.
        let inside = Self { depth: self.depth + 1, activity: self.activity.deeper(), ..self.clone() };
        let idle = plugin.idle.lock().unwrap().pop();
        let (mut store, bindings) = match idle {
            Some(instance) => instance,
            None => inside
                .instantiate(plugin)
                .map_err(|e| CallError::ExecutionFailed(format!("starting plugin '{}': {e}", call.name)))?,
        };
        let state = store.data_mut();
        state.activity = inside.activity.clone();
        if state.plugins.is_some() {
            state.plugins = Some(inside);
        }

        let result = self.activity.agent(&call.name, input, || {
            bindings
                .agentwos_core_agent()
                .call_run(&mut store, &[message("user", input)])
                .map_err(|e| CallError::ExecutionFailed(format!("plugin '{}' crashed: {e}", call.name)))?
        });
        plugin.idle.lock().unwrap().push((store, bindings));
        result
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
    trace: bool,
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

    let activity = Activity::new(trace);
    let plugins = Plugins::load(engine, manifest, &project, &model, &activity)?;
    let mut router = AgentInstance::new(engine, &router, &llm, Box::new(plugins))?;

    if let Some(text) = text {
        let started = Instant::now();
        let reply = router.ask(request(default.as_deref(), &model, text));
        activity.summary(started);
        println!("{}", reply.map_err(|e| anyhow!(e))?);
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
            let started = Instant::now();
            let reply = router.ask(request(default.as_deref(), &model, line));
            activity.summary(started);
            match reply {
                Ok(reply) => println!("\n{reply}\n"),
                Err(e) => println!("\nerror: {e}\n"),
            }
        }
    }
    Ok(())
}
