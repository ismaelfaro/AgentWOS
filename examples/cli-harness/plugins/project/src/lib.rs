//! `project` expert agent: describes the environment the others work in: the
//! operating system, what is in the project folder, which expert agents fit
//! it, and the project's own instructions for agents.
//! Grants: `workspace`, `plugins`; allowed program: `uname`.

use kit::*;

struct Project;

/// Files a project may keep its instructions for agents in.
const INSTRUCTIONS: [&str; 3] = ["AGENTS.md", "CLAUDE.md", "CONTRIBUTING.md"];
const INSTRUCTION_CHARS: usize = 2000;
const MAX_ENTRIES: usize = 40;

impl Guest for Project {
    fn run(input: Vec<Message>) -> Reply {
        let system = workspace::exec("uname", &["-srm".to_string()]).map(|r| r.stdout).unwrap_or_default();
        let entries = workspace::list_files("")?;
        let names: Vec<String> = entries
            .iter()
            .take(MAX_ENTRIES)
            .map(|e| if e.kind == "dir" { format!("{}/", e.path) } else { e.path.clone() })
            .collect();
        let agents: Vec<String> = agents().into_iter().map(|agent| agent.name).collect();

        let mut report = format!(
            "System: {}\nFiles: {}{}\nAgents that fit this project: {}",
            system.trim(),
            if names.is_empty() { "(none yet)".into() } else { names.join("  ") },
            if entries.len() > MAX_ENTRIES { "  ..." } else { "" },
            agents.join(", "),
        );
        if !text(&input).is_empty() {
            report = format!(
                "The project agent only reports on the environment; it runs and changes nothing. \
                 To run code, ask the expert for its language.\n{report}"
            );
        }
        if let Some(name) = INSTRUCTIONS.iter().find(|name| entries.iter().any(|e| e.path == **name)) {
            if let Ok(body) = workspace::read(name) {
                report += &format!("\nInstructions from {name}:\n{}", clip(body.trim(), INSTRUCTION_CHARS));
            }
        }
        Ok(report)
    }
}

kit::export!(Project);
