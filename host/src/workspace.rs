//! The harness workspace: the one folder plugin agents may read, write and
//! run programs in. A plugin only sees the `workspace` import; every path
//! check and every process start happens here, in the host.

use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::CallError;

/// Output kept per stream of a program; the rest is dropped.
const MAX_OUTPUT_BYTES: usize = 256 * 1024;
/// Environment a program started from the workspace gets. Everything else,
/// API keys included, stays with the host.
const PASSED_ENV: [&str; 7] = ["PATH", "HOME", "LANG", "LC_ALL", "TMPDIR", "CARGO_HOME", "RUSTUP_HOME"];

pub struct Entry {
    pub path: String,
    pub kind: &'static str,
    pub bytes: u64,
}

pub struct Exec {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
    allow: Vec<String>,
    timeout: Duration,
}

/// How many folders down a `*.ext` marker is looked for, and where it is not.
const MARKER_DEPTH: usize = 3;
const NOT_SOURCE: [&str; 6] = [".git", "target", "node_modules", "__pycache__", ".venv", "dist"];

fn has_suffix(folder: &Path, suffix: &str, depth: usize) -> bool {
    let Ok(entries) = std::fs::read_dir(folder) else { return false };
    let entries: Vec<_> = entries.flatten().collect();
    let name = |e: &std::fs::DirEntry| e.file_name().to_string_lossy().into_owned();
    entries.iter().any(|e| name(e).ends_with(suffix))
        || depth > 0
            && entries.iter().any(|e| {
                e.file_type().is_ok_and(|t| t.is_dir())
                    && !NOT_SOURCE.contains(&name(e).as_str())
                    && has_suffix(&e.path(), suffix, depth - 1)
            })
}

fn invalid(message: impl Into<String>) -> CallError {
    CallError::InvalidInput(message.into())
}

fn failed(message: impl Into<String>) -> CallError {
    CallError::ExecutionFailed(message.into())
}

impl Workspace {
    /// A workspace in which no program may be run yet; see `allowing`.
    pub fn new(root: PathBuf, timeout: Duration) -> Self {
        let _ = std::fs::create_dir_all(&root);
        let root = root.canonicalize().unwrap_or(root);
        Self { root, allow: Vec::new(), timeout }
    }

    /// The same folder, for a holder that may run these programs.
    pub fn allowing(&self, allow: &[String]) -> Self {
        Self { allow: allow.to_vec(), ..self.clone() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Whether the project has this marker: a name at its top (`Cargo.toml`)
    /// or a suffix (`*.py`) on any file a few folders deep. This is how a
    /// project is recognised.
    pub fn has(&self, marker: &str) -> bool {
        match marker.strip_prefix('*') {
            None => self.root.join(marker).exists(),
            Some(suffix) => has_suffix(&self.root, suffix, MARKER_DEPTH),
        }
    }

    /// Turns a workspace-relative path into a real one, refusing anything that
    /// names a location outside the root.
    fn resolve(&self, relative: &str) -> Result<PathBuf, CallError> {
        let path = Path::new(relative.trim());
        let inside = path
            .components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
        if !inside {
            return Err(invalid("path must be relative and stay inside the workspace"));
        }
        Ok(self.root.join(path))
    }

    /// Follows symlinks and checks the result is still inside the root. For a
    /// path that does not exist yet, checks its nearest existing parent, so a
    /// new file cannot be created through a symlinked directory.
    fn check_inside(&self, path: &Path) -> Result<(), CallError> {
        let existing = path.ancestors().find(|p| p.exists()).unwrap_or(&self.root);
        let real = existing.canonicalize().map_err(|e| failed(format!("{}: {e}", existing.display())))?;
        if real.starts_with(&self.root) {
            Ok(())
        } else {
            Err(invalid("path leaves the workspace"))
        }
    }

    pub fn read(&self, relative: &str) -> Result<String, CallError> {
        let path = self.resolve(relative)?;
        self.check_inside(&path)?;
        std::fs::read_to_string(&path).map_err(|e| failed(format!("reading {relative}: {e}")))
    }

    pub fn write(&self, relative: &str, contents: &str) -> Result<String, CallError> {
        let path = self.resolve(relative)?;
        if path == self.root {
            return Err(invalid("write needs a file path"));
        }
        self.check_inside(&path)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| failed(format!("creating {}: {e}", parent.display())))?;
        }
        std::fs::write(&path, contents).map_err(|e| failed(format!("writing {relative}: {e}")))?;
        Ok(format!("wrote {relative} ({} bytes)", contents.len()))
    }

    pub fn list(&self, relative: &str) -> Result<Vec<Entry>, CallError> {
        let relative = if relative.trim().is_empty() { "." } else { relative };
        let path = self.resolve(relative)?;
        self.check_inside(&path)?;
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(&path).map_err(|e| failed(format!("listing {relative}: {e}")))? {
            let entry = entry.map_err(|e| failed(format!("listing {relative}: {e}")))?;
            let metadata = entry.metadata().map_err(|e| failed(format!("listing {relative}: {e}")))?;
            entries.push(Entry {
                path: entry.file_name().to_string_lossy().into_owned(),
                kind: if metadata.is_dir() { "dir" } else { "file" },
                bytes: metadata.len(),
            });
        }
        entries.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(entries)
    }

    /// Runs an allowlisted program in the workspace, without a shell, with a
    /// minimal environment, and stops it when it runs past the time limit.
    pub fn exec(&self, program: &str, args: &[String]) -> Result<Exec, CallError> {
        if program.contains(['/', '\\']) {
            return Err(invalid("program must be a bare command name"));
        }
        if !self.allow.iter().any(|allowed| allowed == program) {
            let allowed = if self.allow.is_empty() { "none".to_string() } else { self.allow.join(", ") };
            return Err(invalid(format!("'{program}' is not an allowed program (allowed: {allowed})")));
        }

        let mut child = Command::new(program)
            .args(args)
            .current_dir(&self.root)
            .env_clear()
            .envs(PASSED_ENV.iter().filter_map(|k| std::env::var_os(k).map(|v| (k, v))))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| failed(format!("starting {program}: {e}")))?;

        // Read both streams while the program runs, so one that prints a lot
        // cannot stall on a full pipe.
        let stdout = collect(child.stdout.take());
        let stderr = collect(child.stderr.take());

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait().map_err(|e| failed(format!("waiting for {program}: {e}")))? {
                Some(status) => break Some(status),
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                None => thread::sleep(Duration::from_millis(20)),
            }
        };
        let (stdout, stderr) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());

        match status {
            Some(status) => Ok(Exec { status: status.code().unwrap_or(-1), stdout, stderr }),
            None => Err(failed(format!(
                "{program} was stopped after {} s without finishing",
                self.timeout.as_secs()
            ))),
        }
    }
}

/// Reads a stream to its end on a thread, keeping at most `MAX_OUTPUT_BYTES`.
fn collect(stream: Option<impl Read + Send + 'static>) -> thread::JoinHandle<String> {
    thread::spawn(move || {
        let mut kept = Vec::new();
        let mut dropped = false;
        let mut buffer = [0u8; 8192];
        let Some(mut stream) = stream else { return String::new() };
        while let Ok(n) = stream.read(&mut buffer) {
            if n == 0 {
                break;
            }
            let room = MAX_OUTPUT_BYTES.saturating_sub(kept.len());
            kept.extend_from_slice(&buffer[..n.min(room)]);
            dropped |= n > room;
        }
        let mut text = String::from_utf8_lossy(&kept).into_owned();
        if dropped {
            text.push_str(&format!("\n[output cut at {} KB]\n", MAX_OUTPUT_BYTES / 1024));
        }
        text
    })
}
