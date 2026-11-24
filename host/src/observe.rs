//! The kernel's read-only window, on the network. A running kernel serves its
//! ledger on a loopback port and leaves a note saying where, so a dashboard (or
//! any monitor) can be started on its own, at any time, find the kernel and
//! watch it, the way `ps` reads `/proc`.
//!
//! - The port is on 127.0.0.1 and chosen by the system.
//! - Every request needs the kernel's token, and a loopback `Host`.
//! - The note is a file only this user can read, in a folder only this user
//!   can open. It is removed when the kernel ends; a monitor skips and deletes
//!   the note of a kernel that is gone.
//! - Nothing here can change the kernel: there is no route that writes.

use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::PathBuf;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::http::{self, Response};
use crate::ledger::Ledger;

/// Most events one request may return.
const MAX_EVENTS: usize = 4096;

/// A token for a caller that must prove it was told.
pub fn new_token() -> Result<String> {
    let mut bytes = [0u8; 24];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

/// Where running kernels leave their notes: one folder per user.
/// `AGENTWOS_RUN_DIR` moves it, which a test uses to keep to itself.
pub fn run_dir() -> PathBuf {
    let base = std::env::var_os("AGENTWOS_RUN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join(format!("agentwos-{}", unsafe { libc::getuid() })));
    base.join("kernels")
}

fn unix_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64)
}

/// The note of a published kernel. Dropping it removes the note.
pub struct Published {
    note: PathBuf,
}

impl Drop for Published {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.note);
    }
}

pub fn agents_json(ledger: &Ledger) -> Value {
    Value::Array(
        ledger
            .agents()
            .into_iter()
            .map(|a| {
                json!({ "name": a.name, "busy": a.busy, "calls": a.calls, "errors": a.errors, "thoughts": a.thoughts,
                        "files": a.files, "programs": a.programs, "total_ms": a.total_ms,
                        "busy_since_ms": a.busy_since_ms, "last_ms": a.last_ms, "last": a.last })
            })
            .collect(),
    )
}

pub fn edges_json(ledger: &Ledger) -> Value {
    Value::Array(ledger.edges().into_iter().map(|e| json!({ "caller": e.caller, "agent": e.agent, "calls": e.calls, "busy": e.busy })).collect())
}

pub fn events_json(ledger: &Ledger, after: u64, limit: usize) -> Value {
    Value::Array(
        ledger
            .events_after(after, limit.min(MAX_EVENTS))
            .into_iter()
            .map(|e| json!({ "seq": e.seq, "at_ms": e.at_ms, "agent": e.agent, "caller": e.caller, "kind": e.kind,
                             "detail": e.detail, "ms": e.ms, "ok": e.ok }))
            .collect(),
    )
}

/// Serves `ledger` read-only on a loopback port and leaves a note for monitors.
/// `name` says what this kernel runs (the manifest), for people.
pub fn publish(ledger: &Ledger, name: &str) -> Result<Published> {
    let listener = http::listen("127.0.0.1:0")?;
    let addr = listener.local_addr()?;
    let token = new_token()?;
    let pid = std::process::id();
    let started = unix_ms();
    let info = json!({ "id": format!("{pid}-{started}"), "pid": pid, "name": name, "addr": addr.to_string(),
                       "token": token, "started_unix_ms": started });

    let dir = run_dir();
    fs::DirBuilder::new().recursive(true).mode(0o700).create(&dir)?;
    let note = dir.join(format!("{pid}.json"));
    // Written whole under another name, then renamed, so a reader never sees half of it.
    let partial = dir.join(format!(".{pid}.json"));
    OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&partial)?.write_all(info.to_string().as_bytes())?;
    fs::rename(&partial, &note)?;

    let ledger = ledger.clone();
    let public = json!({ "id": info["id"], "pid": pid, "name": name, "started_unix_ms": started });
    thread::spawn(move || {
        let _ = http::serve_with(listener, &token, &[], true, &mut |request| {
            let (path, query) = request.path.split_once('?').unwrap_or((request.path.as_str(), ""));
            let param = |key: &str| query.split('&').find_map(|kv| kv.strip_prefix(key).and_then(|v| v.strip_prefix('='))).and_then(|v| v.parse::<u64>().ok());
            if request.method != "GET" {
                return Response::json(405, &json!({ "error": "this window is read-only" }));
            }
            match path {
                "/observe/info" => Response::json(200, &public),
                "/observe/now" => Response::json(200, &json!({ "now_ms": ledger.now_ms() })),
                "/observe/agents" => Response::json(200, &agents_json(&ledger)),
                "/observe/edges" => Response::json(200, &edges_json(&ledger)),
                "/observe/events" => {
                    let (after, limit) = (param("after").unwrap_or(0), param("limit").unwrap_or(500) as usize);
                    Response::json(200, &events_json(&ledger, after, limit))
                }
                _ => Response::json(404, &json!({ "error": "not found" })),
            }
        });
    });
    Ok(Published { note })
}

/// A running kernel, as a monitor finds it.
#[derive(Clone)]
pub struct Kernel {
    pub id: String,
    pub pid: u32,
    pub name: String,
    pub addr: SocketAddr,
    token: String,
}

impl Kernel {
    pub fn get(&self, path: &str) -> Result<Value> {
        let body = http::get(self.addr, path, &self.token)?;
        serde_json::from_slice(&body).map_err(|e| anyhow!("kernel {}: {e}", self.pid))
    }

    pub fn describe(&self) -> Value {
        json!({ "id": self.id, "pid": self.pid, "name": self.name })
    }
}

pub fn alive(pid: u32) -> bool {
    // Signal 0 checks that the process exists without touching it.
    let found = unsafe { libc::kill(pid as libc::pid_t, 0) } == 0;
    found || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

/// The kernels running now, newest first. Notes of kernels that are gone are
/// removed on the way.
pub fn kernels() -> Vec<Kernel> {
    let mut found: Vec<(u64, Kernel)> = Vec::new();
    let Ok(entries) = fs::read_dir(run_dir()) else { return found.into_iter().map(|(_, k)| k).collect() };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") || path.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')) {
            continue;
        }
        let Some(note) = fs::read(&path).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
        let pid = note["pid"].as_u64().unwrap_or(0) as u32;
        if pid == 0 || !alive(pid) {
            let _ = fs::remove_file(&path);
            continue;
        }
        let (Some(addr), Some(token)) = (note["addr"].as_str().and_then(|a| a.parse().ok()), note["token"].as_str()) else { continue };
        let kernel = Kernel {
            id: note["id"].as_str().unwrap_or("").to_string(),
            pid,
            name: note["name"].as_str().unwrap_or("").to_string(),
            addr,
            token: token.to_string(),
        };
        found.push((note["started_unix_ms"].as_u64().unwrap_or(0), kernel));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found.into_iter().map(|(_, k)| k).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn a_published_kernel_is_found_read_and_forgotten() {
        let dir = std::env::temp_dir().join(format!("agentwos-test-{}", std::process::id()));
        std::env::set_var("AGENTWOS_RUN_DIR", &dir);
        let ledger = Ledger::new();
        ledger.record("echo", "router", "start", "hi", Duration::ZERO, true);
        let published = publish(&ledger, "test").unwrap();

        let note = fs::metadata(run_dir().join(format!("{}.json", std::process::id()))).unwrap();
        assert_eq!(note.permissions().mode() & 0o777, 0o600, "only this user may read the token");

        let kernel = kernels().into_iter().find(|k| k.pid == std::process::id()).expect("found");
        assert_eq!(kernel.get("/observe/agents").unwrap()[0]["name"], "echo");
        assert_eq!(kernel.get("/observe/events?after=0").unwrap().as_array().unwrap().len(), 1);
        let stranger = Kernel { token: "wrong".into(), ..kernel.clone() };
        assert!(stranger.get("/observe/agents").is_err());

        drop(published);
        assert!(kernels().iter().all(|k| k.pid != std::process::id()), "the note goes with the kernel");
        let _ = fs::remove_dir_all(dir);
    }

    use std::os::unix::fs::PermissionsExt;
}
