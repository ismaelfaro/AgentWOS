//! `python` expert agent: runs, tests and checks Python code.
//! Grant: `workspace`, allowed program: `python3`.

use kit::*;

struct Python;

const HELP: &str = "\
run <file> [args]     run a script (or: run -c \"<code>\")
test [args]           run the tests: pytest when installed, unittest otherwise
check <file>          compile a file to find syntax errors";

/// Uses whichever test runner this environment has.
fn test(args: &str) -> Reply {
    let result = run_line("python3", &["-m", "pytest", "-q"], args)?;
    if result.contains("No module named pytest") {
        return run_line("python3", &["-m", "unittest"], args);
    }
    Ok(result)
}

impl Guest for Python {
    fn run(input: Vec<Message>) -> Reply {
        match command(text(&input)) {
            ("" | "help", _) => Ok(HELP.into()),
            ("run" | "python" | "python3", args) => run_line("python3", &[], args),
            ("test", args) => test(args),
            ("check", file) => run_line("python3", &["-m", "py_compile"], file),
            (other, _) => Err(invalid(format!("the python expert has no '{other}' command\n{HELP}"))),
        }
    }
}

kit::export!(Python);
