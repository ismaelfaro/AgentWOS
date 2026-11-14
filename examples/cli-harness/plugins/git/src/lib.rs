//! `git` expert agent: version control for the project.
//! Grant: `workspace`, allowed program: `git`.
//!
//! Its expertise is a policy: it runs the everyday commands and refuses the
//! ones that publish or throw work away, so no other agent can do those.

use kit::*;

struct Git;

const COMMANDS: [&str; 10] = ["status", "diff", "log", "show", "add", "commit", "branch", "switch", "stash", "init"];

impl Guest for Git {
    fn run(input: Vec<Message>) -> Reply {
        let request = text(&input);
        let line = request.strip_prefix("git ").unwrap_or(request);
        match command(line) {
            ("" | "help", _) => Ok(format!("git <command> [args], with command one of: {}", COMMANDS.join(", "))),
            ("log", "") => run_line("git", &["log", "--oneline", "-n", "15"], ""),
            (sub, _) if COMMANDS.contains(&sub) => run_line("git", &[], line),
            (sub, _) => Err(invalid(format!(
                "the git expert does not run '{sub}' (it runs: {}). Publishing and discarding work are left to the user.",
                COMMANDS.join(", ")
            ))),
        }
    }
}

kit::export!(Git);
