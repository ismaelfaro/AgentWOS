//! Serves the dashboard agent to a browser. The agent is a WASM component
//! that can only read what a kernel recorded (`observe`). It runs either inside
//! the kernel it watches, or on its own: then it finds a running kernel through
//! the note the kernel leaves (see `observe.rs`), reads it over loopback, and
//! when that kernel ends it waits for the next one. What counts as trouble, and
//! how it looks, is the component's business, not the host's.

use std::net::SocketAddr;
use std::thread;

use anyhow::Result;
use serde_json::{json, Value};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtxView, WasiView};

use crate::http::{self, Response};
use crate::ledger::Ledger;
use crate::observe::{self as window, Kernel};
use crate::runtime::WasiState;

pub mod world {
    wasmtime::component::bindgen!({ world: "dashboard-module", path: "../wit/world.wit" });
}
use world::agentwos::core::observe;
use world::DashboardModule;

const DASHBOARD: &[u8] = include_bytes!("../../modules/dashboard/target/wasm32-wasip2/release/dashboard.wasm");
/// Most events one read may ask for.
const MAX_EVENTS: usize = 4096;

/// What the dashboard watches.
pub enum Source {
    /// The kernel it runs in.
    Here(Ledger),
    /// A kernel in another process: the one with this pid, or the newest.
    Away { pid: Option<u32>, kernel: Option<Kernel> },
}

impl Source {
    /// The kernel to read, found again if the last one is gone.
    fn kernel(&mut self) -> Option<&Kernel> {
        let Source::Away { pid, kernel } = self else { return None };
        if kernel.as_ref().is_some_and(|k| !window::alive(k.pid)) {
            *kernel = None;
        }
        if kernel.is_none() {
            *kernel = window::kernels().into_iter().find(|k| pid.is_none_or(|p| k.pid == p));
        }
        kernel.as_ref()
    }

    /// One read from the kernel away. A failure forgets the kernel, so the next
    /// read looks for one again.
    fn read(&mut self, path: &str) -> Option<Value> {
        let answer = self.kernel()?.get(path);
        match answer {
            Ok(value) => Some(value),
            Err(_) => {
                if let Source::Away { kernel, .. } = self {
                    *kernel = None;
                }
                None
            }
        }
    }

    fn json(&mut self, path: &str, here: impl Fn(&Ledger) -> Value) -> Value {
        match self {
            Source::Here(ledger) => here(ledger),
            Source::Away { .. } => self.read(path).unwrap_or(Value::Null),
        }
    }
}

/// The Store of the dashboard agent. It holds what it watches and nothing else:
/// no files, no network of its own, no other agent.
struct State {
    wasi: WasiState,
    source: Source,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        self.wasi.ctx()
    }
}

fn text(v: &Value, key: &str) -> String {
    v[key].as_str().unwrap_or("").to_string()
}
fn num(v: &Value, key: &str) -> u64 {
    v[key].as_u64().unwrap_or(0)
}
fn list(v: Value) -> Vec<Value> {
    match v {
        Value::Array(items) => items,
        _ => Vec::new(),
    }
}

impl observe::Host for State {
    fn source(&mut self) -> String {
        match &mut self.source {
            Source::Here(_) => json!({ "state": "connected", "kernel": { "id": format!("here-{}", std::process::id()), "pid": std::process::id(), "name": "this kernel" } }),
            away => match away.kernel().map(Kernel::describe) {
                Some(kernel) => json!({ "state": "connected", "kernel": kernel }),
                None => json!({ "state": "waiting", "looking_in": window::run_dir() }),
            },
        }
        .to_string()
    }

    fn now(&mut self) -> u64 {
        num(&self.source.json("/observe/now", |l| json!({ "now_ms": l.now_ms() })), "now_ms")
    }

    fn agents(&mut self) -> Vec<observe::AgentStat> {
        list(self.source.json("/observe/agents", window::agents_json))
            .iter()
            .map(|a| observe::AgentStat {
                name: text(a, "name"),
                busy: num(a, "busy") as u32,
                calls: num(a, "calls") as u32,
                errors: num(a, "errors") as u32,
                thoughts: num(a, "thoughts") as u32,
                files: num(a, "files") as u32,
                programs: num(a, "programs") as u32,
                total_ms: num(a, "total_ms"),
                busy_since_ms: num(a, "busy_since_ms"),
                last_ms: num(a, "last_ms"),
                last: text(a, "last"),
            })
            .collect()
    }

    fn edges(&mut self) -> Vec<observe::Edge> {
        list(self.source.json("/observe/edges", window::edges_json))
            .iter()
            .map(|e| observe::Edge { caller: text(e, "caller"), agent: text(e, "agent"), calls: num(e, "calls") as u32, busy: num(e, "busy") as u32 })
            .collect()
    }

    fn events(&mut self, after: u64, limit: u32) -> Vec<observe::Event> {
        let limit = (limit as usize).min(MAX_EVENTS);
        list(self.source.json(&format!("/observe/events?after={after}&limit={limit}"), |l| window::events_json(l, after, limit)))
            .iter()
            .map(|e| observe::Event {
                seq: num(e, "seq"),
                at_ms: num(e, "at_ms"),
                agent: text(e, "agent"),
                caller: text(e, "caller"),
                kind: text(e, "kind"),
                detail: text(e, "detail"),
                ms: num(e, "ms"),
                ok: e["ok"].as_bool().unwrap_or(false),
            })
            .collect()
    }
}

/// What `start` hands back: where it listens and how to open it.
pub struct Running {
    pub addr: SocketAddr,
    pub token: String,
}

impl Running {
    pub fn url(&self) -> String {
        format!("http://{}/#token={}", self.addr, self.token)
    }
}

/// Starts the dashboard on a loopback address, on its own thread, and returns
/// at once. It watches `source` until the process ends.
pub fn start(engine: &Engine, addr: &str, source: Source) -> Result<Running> {
    let listener = http::listen(addr)?;
    let local = listener.local_addr()?;
    let token = window::new_token()?;

    let component = Component::new(engine, DASHBOARD)?;
    let mut linker = Linker::<State>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    DashboardModule::add_to_linker::<_, HasSelf<State>>(&mut linker, |s| s)?;
    let mut store = Store::new(engine, State { wasi: WasiState::new(&[]), source });
    let bindings = DashboardModule::instantiate(&mut store, &component, &linker)?;
    let page = bindings.agentwos_core_dashboard().call_page(&mut store)?;

    let secret = token.clone();
    thread::spawn(move || {
        let result = http::serve_with(listener, &secret, &["/"], true, &mut |request| {
            let (path, query) = request.path.split_once('?').unwrap_or((request.path.as_str(), ""));
            let param = |key: &str| query.split('&').find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('=')));
            let dashboard = bindings.agentwos_core_dashboard();
            let failed = |e: wasmtime::Error| Response::json(500, &serde_json::json!({ "error": format!("the dashboard agent failed: {e}") }));
            match (request.method.as_str(), path) {
                ("GET", "/") => Response::html(page.clone()),
                ("GET", "/api/snapshot") => {
                    let after = param("after").and_then(|v| v.parse().ok()).unwrap_or(0);
                    dashboard.call_snapshot(&mut store, after).map(|json| Response::json_text(200, json)).unwrap_or_else(failed)
                }
                ("GET", "/api/report") => {
                    let agent = param("agent").filter(|a| !a.is_empty()).map(str::to_string);
                    dashboard
                        .call_report(&mut store, agent.as_deref())
                        .map(|text| Response::json(200, &serde_json::json!({ "report": text })))
                        .unwrap_or_else(failed)
                }
                (_, "/" | "/api/snapshot" | "/api/report") => Response::json(405, &serde_json::json!({ "error": "method not allowed" })),
                _ => Response::json(404, &serde_json::json!({ "error": "not found" })),
            }
        });
        if let Err(e) = result {
            eprintln!("dashboard stopped: {e}");
        }
    });
    Ok(Running { addr: local, token })
}

/// The dashboard on its own: no agents of its own, it watches whichever kernel
/// is running (the one with `pid`, if given) and waits when there is none.
pub fn run_alone(engine: &Engine, addr: &str, pid: Option<u32>) -> Result<()> {
    let running = start(engine, addr, Source::Away { pid, kernel: None })?;
    eprintln!("dashboard: {}", running.url());
    match window::kernels().into_iter().find(|k| pid.is_none_or(|p| k.pid == p)) {
        Some(k) => eprintln!("watching kernel {} ({})", k.pid, k.name),
        None => eprintln!("no kernel is running yet; the dashboard will connect to the next one that starts"),
    }
    loop {
        thread::park();
    }
}
