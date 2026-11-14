//! What the harness shows while it works. The host mediates every call an
//! agent makes, so it can report all of them without any help from the
//! agents: which agent was asked, when a model is thinking, which files were
//! touched, which program is running and how it ended.
//!
//! Lines go to stderr, indented by how deep the chain of agents is. On a
//! terminal, whatever is being waited for has a spinner and a running clock.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::{error_text, CallError};

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Lines of a program's output shown under it, and their width.
const OUTPUT_LINES: usize = 6;
const LINE_CHARS: usize = 110;

const DIM: &str = "2";
const RED: &str = "31";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const CYAN: &str = "36";
const BOLD: &str = "1";

struct Shared {
    terminal: bool,
    color: bool,
    depth: AtomicUsize,
    agents: AtomicUsize,
    thoughts: AtomicUsize,
    programs: AtomicUsize,
}

/// Cheap to clone: every plugin's state holds one. Silent when created off.
#[derive(Clone)]
pub struct Activity(Option<Arc<Shared>>);

fn seconds(since: Instant) -> String {
    format!("{:.1}s", since.elapsed().as_secs_f32())
}

fn one_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("");
    match line.char_indices().nth(max) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None if text.lines().nth(1).is_some() => format!("{line} …"),
        None => line.to_string(),
    }
}

impl Activity {
    /// Shown on a terminal, or anywhere when `always` is set.
    pub fn new(always: bool) -> Self {
        let terminal = std::io::stderr().is_terminal();
        if !terminal && !always {
            return Self(None);
        }
        let zero = || AtomicUsize::new(0);
        Self(Some(Arc::new(Shared {
            terminal,
            color: terminal && std::env::var_os("NO_COLOR").is_none(),
            depth: zero(),
            agents: zero(),
            thoughts: zero(),
            programs: zero(),
        })))
    }

    fn paint(&self, code: &str, text: &str) -> String {
        match &self.0 {
            Some(s) if s.color => format!("\x1b[{code}m{text}\x1b[0m"),
            _ => text.to_string(),
        }
    }

    fn line(&self, text: &str) {
        if let Some(s) = &self.0 {
            eprintln!("{}{text}", "  ".repeat(s.depth.load(Ordering::Relaxed)));
        }
    }

    /// An agent is asked something. Everything it does until it answers is
    /// shown one level deeper.
    pub fn agent<T>(&self, name: &str, input: &str, work: impl FnOnce() -> Result<T, CallError>) -> Result<T, CallError> {
        let Some(s) = &self.0 else { return work() };
        s.agents.fetch_add(1, Ordering::Relaxed);
        self.line(&format!("{} {} {}", self.paint(CYAN, "●"), self.paint(BOLD, name), self.paint(DIM, &one_line(input, 70))));
        s.depth.fetch_add(1, Ordering::Relaxed);
        let result = work();
        s.depth.fetch_sub(1, Ordering::Relaxed);
        if let Err(e) = &result {
            self.line(&format!("{} {name}: {}", self.paint(RED, "✗"), one_line(error_text(e), LINE_CHARS)));
        }
        result
    }

    /// Something that happens at once: a file read or written.
    pub fn did<T>(&self, what: &str, result: &Result<T, CallError>, detail: impl Fn(&T) -> String) {
        match result {
            Ok(value) => self.line(&format!("{} {what} {}", self.paint(DIM, "·"), self.paint(DIM, &detail(value)))),
            Err(e) => self.line(&format!("{} {what}: {}", self.paint(RED, "✗"), one_line(error_text(e), LINE_CHARS))),
        }
    }

    /// Something worth waiting for. Shows a spinner and a clock while `work`
    /// runs, then one line saying how it went.
    fn wait<T>(&self, what: &str, work: impl FnOnce() -> Result<T, CallError>, outcome: impl Fn(&T) -> (bool, String)) -> Result<T, CallError> {
        let Some(s) = &self.0 else { return work() };
        let indent = "  ".repeat(s.depth.load(Ordering::Relaxed));
        let started = Instant::now();
        let finished = AtomicBool::new(false);
        let result = thread::scope(|scope| {
            if s.terminal {
                scope.spawn(|| {
                    for frame in FRAMES.iter().cycle() {
                        if finished.load(Ordering::Relaxed) {
                            break;
                        }
                        eprint!("\r\x1b[2K{indent}{} {what} {}", self.paint(CYAN, frame), self.paint(DIM, &seconds(started)));
                        let _ = std::io::stderr().flush();
                        thread::sleep(Duration::from_millis(80));
                    }
                    eprint!("\r\x1b[2K");
                });
            }
            let result = work();
            finished.store(true, Ordering::Relaxed);
            result
        });
        let took = self.paint(DIM, &seconds(started));
        match &result {
            Ok(value) => {
                let (good, detail) = outcome(value);
                let mark = if good { self.paint(GREEN, "✓") } else { self.paint(YELLOW, "!") };
                let detail = if detail.is_empty() { detail } else { format!(" {detail}") };
                self.line(&format!("{mark} {what}{detail} {took}"));
            }
            Err(e) => self.line(&format!("{} {what}: {} {took}", self.paint(RED, "✗"), one_line(error_text(e), LINE_CHARS))),
        }
        result
    }

    /// A model is asked for a completion.
    pub fn thinking(&self, model: &str, work: impl FnOnce() -> Result<String, CallError>) -> Result<String, CallError> {
        if let Some(s) = &self.0 {
            s.thoughts.fetch_add(1, Ordering::Relaxed);
        }
        let name = model.rsplit('/').next().unwrap_or(model);
        self.wait(&format!("thinking {}", self.paint(DIM, name)), work, |_| (true, String::new()))
    }

    /// A program runs in the workspace. `parts` gives its exit code and output.
    pub fn program<T>(
        &self,
        command: &str,
        work: impl FnOnce() -> Result<T, CallError>,
        parts: impl Fn(&T) -> (i32, &str, &str),
    ) -> Result<T, CallError> {
        if let Some(s) = &self.0 {
            s.programs.fetch_add(1, Ordering::Relaxed);
        }
        let what = format!("$ {}", one_line(command, 80));
        let result = self.wait(&what, work, |run| {
            let status = parts(run).0;
            (status == 0, if status == 0 { String::new() } else { self.paint(YELLOW, &format!("exit {status}")) })
        });
        if let Ok(run) = &result {
            let (_, stdout, stderr) = parts(run);
            let output = if stdout.trim().is_empty() { stderr } else { stdout };
            let lines: Vec<&str> = output.trim_end().lines().collect();
            let hidden = lines.len().saturating_sub(OUTPUT_LINES);
            if hidden > 0 {
                self.line(&self.paint(DIM, &format!("│ … {hidden} more lines")));
            }
            for line in &lines[hidden..] {
                self.line(&self.paint(DIM, &format!("│ {}", one_line(line, LINE_CHARS))));
            }
        }
        result
    }

    /// One line closing a request: what it took. Starts the count again.
    pub fn summary(&self, started: Instant) {
        let Some(s) = &self.0 else { return };
        let count = |n: &AtomicUsize, one: &str, many: &str| match n.swap(0, Ordering::Relaxed) {
            0 => None,
            1 => Some(format!("1 {one}")),
            n => Some(format!("{n} {many}")),
        };
        let parts: Vec<String> = [
            count(&s.agents, "agent call", "agent calls"),
            count(&s.thoughts, "model call", "model calls"),
            count(&s.programs, "program", "programs"),
            Some(seconds(started)),
        ]
        .into_iter()
        .flatten()
        .collect();
        eprintln!("{}", self.paint(DIM, &format!("  {}", parts.join(" · "))));
    }
}
