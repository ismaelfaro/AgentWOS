//! What the harness shows while it works. The host mediates every call an
//! agent makes, so it can report all of them without any help from the
//! agents: which agent was asked, when a model is thinking, which files were
//! touched, which program is running and how it ended.
//!
//! Lines go to stderr, indented by how deep the chain of agents is. Agents
//! working at the same time each get a branch number in front of their lines.
//! On a terminal, a last line with a spinner and a clock says what is being
//! waited for right now.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
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
const MAGENTA: &str = "35";
const CYAN: &str = "36";
const BOLD: &str = "1";

/// One thing being waited for.
struct Waiting {
    id: usize,
    what: String,
    since: Instant,
}

struct Shared {
    terminal: bool,
    color: bool,
    /// Everything in flight. Also the lock for writing to the screen, so a
    /// line and the spinner never overlap.
    waiting: Mutex<Vec<Waiting>>,
    next_id: AtomicUsize,
    agents: AtomicUsize,
    thoughts: AtomicUsize,
    programs: AtomicUsize,
}

/// A place in the tree of calls: how deep, and on which parallel branch.
/// Cheap to clone. Silent when created off.
#[derive(Clone)]
pub struct Activity {
    shared: Option<Arc<Shared>>,
    depth: usize,
    branch: String,
}

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

fn paint(shared: &Shared, code: &str, text: &str) -> String {
    if shared.color {
        format!("\x1b[{code}m{text}\x1b[0m")
    } else {
        text.to_string()
    }
}

/// The spinner line: the oldest thing in flight, and how many more there are.
fn draw_status(shared: &Shared, waiting: &[Waiting], frame: &str) {
    let Some(first) = waiting.first() else { return };
    let more = match waiting.len() - 1 {
        0 => String::new(),
        n => paint(shared, MAGENTA, &format!("  +{n} more at the same time")),
    };
    eprint!("\r\x1b[2K{} {} {}{more}", paint(shared, CYAN, frame), first.what, paint(shared, DIM, &seconds(first.since)));
    let _ = std::io::stderr().flush();
}

impl Activity {
    /// Shown on a terminal, or anywhere when `always` is set.
    pub fn new(always: bool) -> Self {
        let terminal = std::io::stderr().is_terminal();
        if !terminal && !always {
            return Self { shared: None, depth: 0, branch: String::new() };
        }
        let zero = || AtomicUsize::new(0);
        let shared = Arc::new(Shared {
            terminal,
            color: terminal && std::env::var_os("NO_COLOR").is_none(),
            waiting: Mutex::new(Vec::new()),
            next_id: zero(),
            agents: zero(),
            thoughts: zero(),
            programs: zero(),
        });
        if terminal {
            let shared = shared.clone();
            thread::spawn(move || {
                for frame in FRAMES.iter().cycle() {
                    thread::sleep(Duration::from_millis(80));
                    draw_status(&shared, &shared.waiting.lock().unwrap(), frame);
                }
            });
        }
        Self { shared: Some(shared), depth: 0, branch: String::new() }
    }

    /// One level further in: where the things an agent does are shown.
    pub fn deeper(&self) -> Self {
        Self { depth: self.depth + 1, ..self.clone() }
    }

    /// One of several agents working at the same time.
    pub fn branch(&self, number: usize) -> Self {
        let branch = if self.branch.is_empty() { number.to_string() } else { format!("{}.{number}", self.branch) };
        Self { branch, ..self.clone() }
    }

    fn paint(&self, code: &str, text: &str) -> String {
        self.shared.as_ref().map_or(text.to_string(), |s| paint(s, code, text))
    }

    fn prefix(&self) -> String {
        let branch = if self.branch.is_empty() { String::new() } else { self.paint(MAGENTA, &format!("[{}] ", self.branch)) };
        format!("{}{branch}", "  ".repeat(self.depth))
    }

    fn line(&self, text: &str) {
        let Some(s) = &self.shared else { return };
        let waiting = s.waiting.lock().unwrap();
        if s.terminal {
            eprint!("\r\x1b[2K");
        }
        eprintln!("{}{text}", self.prefix());
        if s.terminal {
            draw_status(s, &waiting, FRAMES[0]);
        }
    }

    /// An agent is asked something. What it does until it answers is shown
    /// by `self.deeper()`.
    pub fn agent<T>(&self, name: &str, input: &str, work: impl FnOnce() -> Result<T, CallError>) -> Result<T, CallError> {
        let Some(s) = &self.shared else { return work() };
        s.agents.fetch_add(1, Ordering::Relaxed);
        self.line(&format!("{} {} {}", self.paint(CYAN, "●"), self.paint(BOLD, name), self.paint(DIM, &one_line(input, 70))));
        let result = work();
        if let Err(e) = &result {
            self.line(&format!("{} {name}: {}", self.paint(RED, "✗"), one_line(error_text(e), LINE_CHARS)));
        }
        result
    }

    /// Several agents are asked at the same time.
    pub fn together(&self, count: usize) {
        self.line(&self.paint(MAGENTA, &format!("⇉ {count} agents at the same time")));
    }

    /// Something that happens at once: a file read or written.
    pub fn did<T>(&self, what: &str, result: &Result<T, CallError>, detail: impl Fn(&T) -> String) {
        match result {
            Ok(value) => self.line(&format!("{} {what} {}", self.paint(DIM, "·"), self.paint(DIM, &detail(value)))),
            Err(e) => self.line(&format!("{} {what}: {}", self.paint(RED, "✗"), one_line(error_text(e), LINE_CHARS))),
        }
    }

    /// Something worth waiting for. It is on the spinner line while `work`
    /// runs, then gets one line saying how it went.
    fn wait<T>(
        &self,
        what: &str,
        work: impl FnOnce() -> Result<T, CallError>,
        outcome: impl Fn(&T) -> (bool, String),
    ) -> Result<T, CallError> {
        let Some(s) = &self.shared else { return work() };
        let id = s.next_id.fetch_add(1, Ordering::Relaxed);
        let since = Instant::now();
        s.waiting.lock().unwrap().push(Waiting { id, what: format!("{}{what}", self.prefix()), since });
        let result = work();
        s.waiting.lock().unwrap().retain(|w| w.id != id);

        let took = self.paint(DIM, &seconds(since));
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
        if let Some(s) = &self.shared {
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
        if let Some(s) = &self.shared {
            s.programs.fetch_add(1, Ordering::Relaxed);
        }
        let what = format!("$ {}", one_line(command, 80));
        let result = self.wait(&what, work, |run| match parts(run).0 {
            0 => (true, String::new()),
            status => (false, self.paint(YELLOW, &format!("exit {status}"))),
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
        let Some(s) = &self.shared else { return };
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
        self.line(&self.paint(DIM, &format!("  {}", parts.join(" · "))));
    }
}
