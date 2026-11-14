//! `workspace` plugin agent: reads, writes, lists and runs things in the
//! harness workspace.
//!
//! It needs the `workspace` grant. The component has no filesystem or process
//! access of its own: every command becomes a call to the host's `workspace`
//! import, which keeps paths inside the workspace and only starts allowed programs.

wit_bindgen::generate!({
    world: "plugin-module",
    path: "../../../../wit/world.wit",
});

use agentwos::core::types::{CallError, Message};
use agentwos::core::workspace;
use exports::agentwos::core::agent::Guest;

struct Component;

const HELP: &str = "\
write <path> <text>      write a one-line file
write <path>             write a file from the lines that follow
read <path>              show a file
list [path]              list a folder
run <program> [args]     run an allowed program; quotes group words";

fn invalid(message: &str) -> CallError {
    CallError::InvalidInput(message.into())
}

/// Splits a command line into words. Single and double quotes group words, and
/// a backslash keeps the next character as it is. No shell is involved.
fn words(line: &str) -> Result<Vec<String>, CallError> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut started = false;
    let mut quote: Option<char> = None;
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some('\''), c) => word.push(c),
            (_, '\\') => word.push(chars.next().ok_or_else(|| invalid("a backslash needs a character after it"))?),
            (Some(_), c) => word.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                started = true;
            }
            (None, c) if c.is_whitespace() => {
                if started || !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            (None, c) => word.push(c),
        }
    }
    if quote.is_some() {
        return Err(invalid("a quote is never closed"));
    }
    if started || !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

fn write(rest: &str) -> Result<String, CallError> {
    // The content is taken exactly as given: nothing is unescaped.
    let (path, content) = match rest.split_once('\n') {
        Some((path, content)) => (path.trim(), content),
        None => rest.split_once(' ').unwrap_or((rest, "")),
    };
    if path.is_empty() {
        return Err(invalid("write needs a path"));
    }
    workspace::write(path, content)
}

fn list(path: &str) -> Result<String, CallError> {
    let entries = workspace::list_files(path)?;
    if entries.is_empty() {
        return Ok("(empty)".into());
    }
    Ok(entries
        .iter()
        .map(|e| format!("{:<4} {:>8}  {}", e.kind, e.bytes, e.path))
        .collect::<Vec<_>>()
        .join("\n"))
}

fn run(command: &str) -> Result<String, CallError> {
    let words = words(command)?;
    let (program, args) = words.split_first().ok_or_else(|| invalid("run needs a program"))?;
    let result = workspace::exec(program, args)?;
    let mut out = format!("exit {}", result.status);
    for (label, stream) in [("", &result.stdout), ("stderr:\n", &result.stderr)] {
        if !stream.is_empty() {
            out += &format!("\n{label}{}", stream.trim_end());
        }
    }
    Ok(out)
}

impl Guest for Component {
    fn run(input: Vec<Message>) -> Result<String, CallError> {
        let text = input.last().map(|m| m.content.trim()).unwrap_or("");
        let (command, rest) = text.split_once(char::is_whitespace).unwrap_or((text, ""));
        match command {
            "" | "help" => Ok(HELP.into()),
            "write" => write(rest),
            "read" => workspace::read(rest.trim()),
            "list" => list(rest.trim()),
            "run" => run(rest),
            other => Err(CallError::InvalidInput(format!("workspace has no '{other}' command\n{HELP}"))),
        }
    }
}

export!(Component);
