//! `files` plugin agent: reads, writes and lists files of the project.
//! Grant: `workspace`. The host keeps every path inside the project folder.

use kit::*;

struct Files;

const HELP: &str = "\
write <path> <text>      write a one-line file
write <path>             write a file from the lines that follow
read <path>              show a file
list [path]              list a folder";

fn write(rest: &str) -> Reply {
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

fn list(path: &str) -> Reply {
    let entries = workspace::list_files(path)?;
    if entries.is_empty() {
        return Ok("(empty)".into());
    }
    Ok(entries.iter().map(|e| format!("{:<4} {:>8}  {}", e.kind, e.bytes, e.path)).collect::<Vec<_>>().join("\n"))
}

impl Guest for Files {
    fn run(input: Vec<Message>) -> Reply {
        match command(text(&input)) {
            ("" | "help", _) => Ok(HELP.into()),
            ("write", rest) => write(rest),
            ("read", path) => workspace::read(path),
            ("list", path) => list(path),
            (other, _) => Err(invalid(format!("files has no '{other}' command\n{HELP}"))),
        }
    }
}

kit::export!(Files);
