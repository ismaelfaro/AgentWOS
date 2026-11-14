//! `node` expert agent: runs JavaScript and the project's npm scripts.
//! Grant: `workspace`, allowed programs: `node`, `npm`.

use kit::*;

struct Node;

const HELP: &str = "\
run <file> [args]     run a script with node
test                  run the project's tests (npm test)
script <name>         run a script of package.json (npm run <name>)";

impl Guest for Node {
    fn run(input: Vec<Message>) -> Reply {
        match command(text(&input)) {
            ("" | "help", _) => Ok(HELP.into()),
            ("run" | "node", args) => run_line("node", &[], args),
            ("test", _) => run_line("npm", &["test", "--silent"], ""),
            ("script", name) => run_line("npm", &["run", "--silent"], name),
            (other, _) => Err(invalid(format!("the node expert has no '{other}' command\n{HELP}"))),
        }
    }
}

kit::export!(Node);
