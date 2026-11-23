//! Serves the dashboard agent to a browser. The agent is a WASM component
//! that can only read the kernel's ledger (`observe`); this file gives it that
//! read, and carries its page and its JSON to a loopback address. What counts
//! as trouble, and how it looks, is the component's business, not the host's.

use std::io::Read;
use std::net::SocketAddr;
use std::thread;

use anyhow::{anyhow, Result};
use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Engine, Store};
use wasmtime_wasi::{WasiCtxView, WasiView};

use crate::http::{self, Response};
use crate::ledger::Ledger;
use crate::runtime::WasiState;

pub mod world {
    wasmtime::component::bindgen!({ world: "dashboard-module", path: "../wit/world.wit" });
}
use world::agentwos::core::observe;
use world::DashboardModule;

const DASHBOARD: &[u8] = include_bytes!("../../modules/dashboard/target/wasm32-wasip2/release/dashboard.wasm");
/// Most events one read may ask for.
const MAX_EVENTS: usize = 4096;

/// The Store of the dashboard agent. It holds the ledger and nothing else: no
/// files, no network, no other agent.
struct State {
    wasi: WasiState,
    ledger: Ledger,
}

impl WasiView for State {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        self.wasi.ctx()
    }
}

impl observe::Host for State {
    fn now(&mut self) -> u64 {
        self.ledger.now_ms()
    }

    fn agents(&mut self) -> Vec<observe::AgentStat> {
        self.ledger
            .agents()
            .into_iter()
            .map(|a| observe::AgentStat {
                name: a.name,
                busy: a.busy,
                calls: a.calls,
                errors: a.errors,
                thoughts: a.thoughts,
                files: a.files,
                programs: a.programs,
                total_ms: a.total_ms,
                busy_since_ms: a.busy_since_ms,
                last_ms: a.last_ms,
                last: a.last,
            })
            .collect()
    }

    fn edges(&mut self) -> Vec<observe::Edge> {
        self.ledger.edges().into_iter().map(|e| observe::Edge { caller: e.caller, agent: e.agent, calls: e.calls, busy: e.busy }).collect()
    }

    fn events(&mut self, after: u64, limit: u32) -> Vec<observe::Event> {
        self.ledger
            .events_after(after, (limit as usize).min(MAX_EVENTS))
            .into_iter()
            .map(|e| observe::Event {
                seq: e.seq,
                at_ms: e.at_ms,
                agent: e.agent,
                caller: e.caller,
                kind: e.kind.to_string(),
                detail: e.detail,
                ms: e.ms,
                ok: e.ok,
            })
            .collect()
    }
}

/// A token a browser must send to read anything but the page.
fn new_token() -> Result<String> {
    let mut bytes = [0u8; 24];
    std::fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
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
/// at once. It watches `ledger` until the process ends.
pub fn start(engine: &Engine, addr: &str, ledger: Ledger) -> Result<Running> {
    let listener = http::listen(addr)?;
    let local = listener.local_addr()?;
    let token = new_token()?;

    let component = Component::new(engine, DASHBOARD)?;
    let mut linker = Linker::<State>::new(engine);
    wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
    DashboardModule::add_to_linker::<_, HasSelf<State>>(&mut linker, |s| s)?;
    let mut store = Store::new(engine, State { wasi: WasiState::new(&[]), ledger });
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
    Ok(Running { addr: local, token }).map_err(|e: anyhow::Error| anyhow!("dashboard: {e}"))
}
