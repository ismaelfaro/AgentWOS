//! What the harness shows while it works. The host mediates every call an
//! agent makes, so it can report all of them without any help from the
//! agents: which agent was asked, when a model is thinking, which files were
//! touched, which program is running and how it ended.
//!
//! Work done in sequence is a tree of lines. Agents working at the same time
//! are a board instead: one row each, redrawn in place, saying what that
//! agent is doing right now. When they have all finished, each row stays as
//! one line with what it took. `--trace` shows every line of every agent
//! instead of the board, and is what a pipe gets.

use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use crate::runtime::{error_text, CallError};

const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
/// Lines of a program's output shown under it.
const OUTPUT_LINES: usize = 6;
/// Width assumed when the terminal does not say.
const DEFAULT_WIDTH: usize = 100;

const DIM: &str = "2";
const BOLD: &str = "1";
const RED: &str = "31";
const GREEN: &str = "32";
const YELLOW: &str = "33";
const MAGENTA: &str = "35";
const CYAN: &str = "36";
/// One colour per parallel agent, in turn.
const BRANCH_COLORS: [&str; 5] = ["35", "36", "33", "34", "32"];

/// Something the main line of work is waiting for.
struct Waiting {
    id: usize,
    depth: usize,
    what: String,
    since: Instant,
}

/// One of several agents working at the same time.
struct Row {
    key: String,
    depth: usize,
    agent: String,
    task: String,
    /// What it is doing now, or did last.
    status: String,
    busy: bool,
    since: Instant,
    started: Instant,
    /// How it ended: whether well, and after how long.
    ended: Option<(bool, Duration)>,
    thoughts: usize,
    programs: usize,
    files: usize,
}

/// The part of the screen that is redrawn: the board, then the spinner line.
#[derive(Default)]
struct Screen {
    rows: Vec<Row>,
    waiting: Vec<Waiting>,
    /// Lines drawn last time, to be erased before anything else is written.
    drawn: usize,
    frame: usize,
    /// Columns of the terminal, asked for again every couple of seconds.
    width: usize,
}

struct Shared {
    terminal: bool,
    color: bool,
    /// Whether parallel agents are shown as a board. Otherwise as lines.
    board: bool,
    screen: Mutex<Screen>,
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

fn elapsed(time: Duration) -> String {
    match time.as_secs() {
        0..=59 => format!("{:.1}s", time.as_secs_f32()),
        s => format!("{}m{:02}s", s / 60, s % 60),
    }
}

/// The first line of a text, cut to `max` characters.
fn one_line(text: &str, max: usize) -> String {
    let line = text.lines().next().unwrap_or("").trim();
    match line.char_indices().nth(max.max(1)) {
        Some((end, _)) => format!("{}…", &line[..end]),
        None => line.to_string(),
    }
}

/// A text without the colour codes in it.
fn plain(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn width() -> usize {
    let size = std::process::Command::new("stty").arg("size").stdin(std::process::Stdio::inherit()).output();
    let columns = size.ok().and_then(|o| String::from_utf8(o.stdout).ok()?.split_whitespace().nth(1)?.parse().ok());
    columns.filter(|c| *c >= 40).unwrap_or(DEFAULT_WIDTH)
}

fn plural(n: usize, one: &str, many: &str) -> Option<String> {
    match n {
        0 => None,
        1 => Some(format!("1 {one}")),
        n => Some(format!("{n} {many}")),
    }
}

impl Shared {
    fn paint(&self, code: &str, text: &str) -> String {
        if self.color {
            format!("\x1b[{code}m{text}\x1b[0m")
        } else {
            text.to_string()
        }
    }

    /// The guide lines in front of something `depth` agents deep.
    fn guides(&self, depth: usize) -> String {
        self.paint(DIM, &"│  ".repeat(depth))
    }

    fn row_line(&self, row: &Row, frame: &str, width: usize) -> String {
        let number = row.key.parse::<usize>().unwrap_or(1);
        let key = self.paint(BRANCH_COLORS[(number - 1) % BRANCH_COLORS.len()], &row.key);
        let (mark, time) = match row.ended {
            Some((true, took)) => (self.paint(GREEN, "✓"), took),
            Some((false, took)) => (self.paint(RED, "✗"), took),
            None if row.busy => (self.paint(CYAN, frame), row.since.elapsed()),
            None => (self.paint(DIM, "·"), row.since.elapsed()),
        };
        let status = match row.ended {
            Some((true, _)) => [
                plural(row.thoughts, "model call", "model calls"),
                plural(row.files, "file", "files"),
                plural(row.programs, "program", "programs"),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
            _ => row.status.clone(),
        };
        let time = elapsed(time);
        // Everything is cut to fit one line: a wrapped row could not be redrawn.
        let fixed = row.depth * 3 + 2 + row.key.len() + 3 + row.agent.len() + 1 + 2 + time.len() + 1;
        let room = width.saturating_sub(fixed + 2);
        // The task column is the same width on every row, so statuses line up.
        let column = (room * 45 / 100).max(12);
        let task = format!("{:<1$}", one_line(&row.task, column - 1), column);
        let status = one_line(&status, room.saturating_sub(column).max(8));
        let status = if matches!(row.ended, Some((false, _))) { self.paint(RED, &status) } else { status };
        format!(
            "{}{} {key} {mark} {} {}  {status} {}",
            self.guides(row.depth),
            self.paint(DIM, "├"),
            self.paint(BOLD, &row.agent),
            self.paint(DIM, &task),
            self.paint(DIM, &time)
        )
    }

    fn erase(&self, screen: &mut Screen) {
        if screen.drawn > 0 {
            eprint!("\r");
            if screen.drawn > 1 {
                eprint!("\x1b[{}A", screen.drawn - 1);
            }
            eprint!("\x1b[J");
            screen.drawn = 0;
        }
    }

    fn draw(&self, screen: &mut Screen) {
        if !self.terminal {
            return;
        }
        if screen.width == 0 || screen.frame % 20 == 0 {
            screen.width = width();
        }
        let width = screen.width;
        let frame = FRAMES[screen.frame % FRAMES.len()];
        let mut lines: Vec<String> = screen.rows.iter().map(|row| self.row_line(row, frame, width)).collect();
        if let Some(first) = screen.waiting.first() {
            let time = elapsed(first.since.elapsed());
            let more = match screen.waiting.len() - 1 {
                0 => String::new(),
                n => format!("  +{n} more"),
            };
            let room = width.saturating_sub(first.depth * 3 + 4 + time.len() + more.len());
            lines.push(format!(
                "{}{} {} {}{}",
                self.guides(first.depth),
                self.paint(CYAN, frame),
                one_line(&first.what, room),
                self.paint(DIM, &time),
                self.paint(MAGENTA, &more)
            ));
        }
        self.erase(screen);
        eprint!("{}", lines.join("\n"));
        let _ = std::io::stderr().flush();
        screen.drawn = lines.len();
    }

    /// Writes lines that stay, above whatever is being redrawn.
    fn print(&self, lines: &[String]) {
        let mut screen = self.screen.lock().unwrap();
        self.erase(&mut screen);
        for line in lines {
            eprintln!("{line}");
        }
        self.draw(&mut screen);
    }
}

impl Activity {
    /// Shown on a terminal, where parallel agents are a board; `trace` shows
    /// it anywhere, as lines only.
    pub fn new(trace: bool) -> Self {
        let terminal = std::io::stderr().is_terminal();
        if !terminal && !trace {
            return Self { shared: None, depth: 0, branch: String::new() };
        }
        let zero = || AtomicUsize::new(0);
        let shared = Arc::new(Shared {
            terminal,
            color: terminal && std::env::var_os("NO_COLOR").is_none(),
            board: terminal && !trace,
            screen: Mutex::new(Screen::default()),
            next_id: zero(),
            agents: zero(),
            thoughts: zero(),
            programs: zero(),
        });
        if terminal {
            let shared = shared.clone();
            thread::spawn(move || loop {
                thread::sleep(Duration::from_millis(100));
                let mut screen = shared.screen.lock().unwrap();
                screen.frame += 1;
                if !screen.rows.is_empty() || !screen.waiting.is_empty() {
                    shared.draw(&mut screen);
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
        self.shared.as_ref().map_or(text.to_string(), |s| s.paint(code, text))
    }

    /// Changes this branch's row of the board. False when there is no board,
    /// so the caller writes a line instead.
    fn on_row(&self, change: impl FnOnce(&mut Row)) -> bool {
        let Some(s) = self.shared.as_ref().filter(|s| s.board && !self.branch.is_empty()) else { return false };
        let key = self.branch.split('.').next().unwrap_or("");
        if let Some(row) = s.screen.lock().unwrap().rows.iter_mut().find(|row| row.key == key) {
            change(row);
        }
        true
    }

    /// Says something happened: on this branch's row, or as a line that stays.
    fn line(&self, text: &str) {
        let Some(s) = &self.shared else { return };
        let status = plain(text);
        if self.on_row(|row| (row.status, row.busy, row.since) = (status, false, Instant::now())) {
            return;
        }
        let branch = match self.branch.as_str() {
            "" => String::new(),
            branch => s.paint(MAGENTA, &format!("{branch} ")),
        };
        s.print(&[format!("{}{branch}{text}", s.guides(self.depth))]);
    }

    /// A line of detail, which a row of the board has no room for.
    fn detail(&self, text: &str) {
        if !self.on_row(|_| ()) {
            self.line(&self.paint(DIM, text));
        }
    }

    /// An agent is asked something. What it does until it answers is shown
    /// by `self.deeper()`.
    pub fn agent<T>(&self, name: &str, input: &str, work: impl FnOnce() -> Result<T, CallError>) -> Result<T, CallError> {
        let Some(s) = &self.shared else { return work() };
        s.agents.fetch_add(1, Ordering::Relaxed);
        // The agent a row is about is the row itself, not something on it.
        let mut owns_row = false;
        self.on_row(|row| owns_row = row.depth == self.depth && row.key == self.branch);
        if !owns_row {
            self.line(&format!("{} {} {}", self.paint(CYAN, "●"), self.paint(BOLD, name), self.paint(DIM, &one_line(input, 72))));
        }
        let result = work();
        if owns_row {
            self.on_row(|row| {
                row.ended = Some((result.is_ok(), row.started.elapsed()));
                if let Err(e) = &result {
                    row.status = one_line(error_text(e), 200);
                }
            });
        } else if let Err(e) = &result {
            self.line(&format!("{} {name}: {}", self.paint(RED, "✗"), one_line(error_text(e), 110)));
        }
        result
    }

    /// Several agents are about to be asked at the same time: (agent, input)
    /// each. Their activity is `self.branch(1)`, `self.branch(2)` and so on.
    pub fn together(&self, asks: &[(String, String)]) {
        let Some(s) = &self.shared else { return };
        self.line(&self.paint(MAGENTA, &format!("⇉ {} agents in parallel", asks.len())));
        if s.board && self.branch.is_empty() {
            let now = Instant::now();
            let row = |(i, (agent, task)): (usize, &(String, String))| Row {
                key: (i + 1).to_string(),
                depth: self.depth,
                agent: agent.clone(),
                task: task.clone(),
                status: "starting".into(),
                busy: true,
                since: now,
                started: now,
                ended: None,
                thoughts: 0,
                programs: 0,
                files: 0,
            };
            s.screen.lock().unwrap().rows = asks.iter().enumerate().map(row).collect();
        }
    }

    /// They have all answered: the board becomes lines that stay.
    pub fn settled(&self) {
        let Some(s) = self.shared.as_ref().filter(|s| s.board && self.branch.is_empty()) else { return };
        let width = width();
        let rows = std::mem::take(&mut s.screen.lock().unwrap().rows);
        s.print(&rows.iter().map(|row| s.row_line(row, "·", width)).collect::<Vec<_>>());
    }

    /// Something that happens at once: a file read or written.
    pub fn did<T>(&self, what: &str, result: &Result<T, CallError>, detail: impl Fn(&T) -> String) {
        match result {
            Ok(value) => {
                self.on_row(|row| row.files += 1);
                self.line(&format!("{} {what} {}", self.paint(DIM, "·"), self.paint(DIM, &detail(value))));
            }
            Err(e) => self.line(&format!("{} {what}: {}", self.paint(RED, "✗"), one_line(error_text(e), 110))),
        }
    }

    /// Something worth waiting for. It has a spinner and a clock while `work`
    /// runs, then one line saying how it went.
    fn wait<T>(
        &self,
        what: &str,
        work: impl FnOnce() -> Result<T, CallError>,
        outcome: impl Fn(&T) -> (bool, String),
    ) -> Result<T, CallError> {
        let Some(s) = &self.shared else { return work() };
        let since = Instant::now();
        let id = s.next_id.fetch_add(1, Ordering::Relaxed);
        let on_row = self.on_row(|row| (row.status, row.busy, row.since) = (plain(what), true, since));
        if !on_row {
            let branch = if self.branch.is_empty() { String::new() } else { format!("{} ", self.branch) };
            let waiting = Waiting { id, depth: self.depth, what: format!("{branch}{}", plain(what)), since };
            s.screen.lock().unwrap().waiting.push(waiting);
        }
        let result = work();
        if !on_row {
            s.screen.lock().unwrap().waiting.retain(|w| w.id != id);
        }

        let took = self.paint(DIM, &elapsed(since.elapsed()));
        match &result {
            Ok(value) => {
                let (good, detail) = outcome(value);
                let mark = if good { self.paint(GREEN, "✓") } else { self.paint(YELLOW, "!") };
                let detail = if detail.is_empty() { detail } else { format!(" {detail}") };
                self.line(&format!("{mark} {what}{detail} {took}"));
            }
            Err(e) => self.line(&format!("{} {what}: {} {took}", self.paint(RED, "✗"), one_line(error_text(e), 110))),
        }
        result
    }

    /// A model is asked for a completion.
    pub fn thinking(&self, model: &str, work: impl FnOnce() -> Result<String, CallError>) -> Result<String, CallError> {
        if let Some(s) = &self.shared {
            s.thoughts.fetch_add(1, Ordering::Relaxed);
        }
        self.on_row(|row| row.thoughts += 1);
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
        self.on_row(|row| row.programs += 1);
        let what = format!("$ {}", one_line(command, 80));
        let result = self.wait(&what, work, |run| match parts(run).0 {
            0 => (true, String::new()),
            status => (false, self.paint(YELLOW, &format!("exit {status}"))),
        });
        if let Ok(run) = &result {
            let (_, stdout, stderr) = parts(run);
            let output = if stdout.trim().is_empty() { stderr } else { stdout };
            let lines: Vec<&str> = output.trim_end().lines().filter(|line| !line.trim().is_empty()).collect();
            let hidden = lines.len().saturating_sub(OUTPUT_LINES);
            if hidden > 0 {
                self.detail(&format!("  ┊ … {hidden} lines before"));
            }
            for line in &lines[hidden..] {
                self.detail(&format!("  ┊ {}", one_line(line, 110)));
            }
        }
        result
    }

    /// One line closing a request: what it took. Starts the count again.
    pub fn summary(&self, started: Instant) {
        let Some(s) = &self.shared else { return };
        let count = |n: &AtomicUsize, one: &str, many: &str| plural(n.swap(0, Ordering::Relaxed), one, many);
        let parts: Vec<String> = [
            count(&s.agents, "agent call", "agent calls"),
            count(&s.thoughts, "model call", "model calls"),
            count(&s.programs, "program", "programs"),
            Some(elapsed(started.elapsed())),
        ]
        .into_iter()
        .flatten()
        .collect();
        self.line(&self.paint(DIM, &format!("└─ {}", parts.join(" · "))));
    }
}
