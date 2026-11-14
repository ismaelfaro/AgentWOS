//! `rust` expert agent: builds, tests and checks a Cargo project.
//! Grant: `workspace`, allowed program: `cargo`.

use kit::*;

struct Rust;

const COMMANDS: [&str; 7] = ["check", "build", "test", "run", "clippy", "fmt", "init"];

impl Guest for Rust {
    fn run(input: Vec<Message>) -> Reply {
        let request = text(&input);
        let line = request.strip_prefix("cargo ").unwrap_or(request);
        match command(line) {
            ("" | "help", _) => Ok(format!("<command> [args], with command one of: {}", COMMANDS.join(", "))),
            (sub, _) if COMMANDS.contains(&sub) => run_line("cargo", &[], line),
            (sub, _) => Err(invalid(format!("the rust expert does not run '{sub}' (it runs: {})", COMMANDS.join(", ")))),
        }
    }
}

kit::export!(Rust);
