//! `search` expert agent: finds text in the project's files.
//! Grant: `workspace`, allowed program: `grep`.

use kit::*;

struct Search;

/// Matches shown; a search that finds more should be narrowed.
const MAX_LINES: usize = 60;
const SKIP: [&str; 4] = ["--exclude-dir=.git", "--exclude-dir=target", "--exclude-dir=node_modules", "--exclude-dir=__pycache__"];

impl Guest for Search {
    fn run(input: Vec<Message>) -> Reply {
        let words = words(text(&input))?;
        let Some((pattern, paths)) = words.split_first() else {
            return Ok("usage: search <pattern> [path]   (pattern is a grep regular expression)".into());
        };
        let mut args: Vec<String> = ["-rnI"].iter().chain(&SKIP).map(|a| a.to_string()).collect();
        args.extend(["-e".to_string(), pattern.clone()]);
        args.extend(if paths.is_empty() { vec![".".to_string()] } else { paths.to_vec() });

        let found = workspace::exec("grep", &args)?;
        if found.stdout.is_empty() {
            return Ok(format!("no matches for '{pattern}'{}", found.stderr.trim_end()));
        }
        let lines: Vec<&str> = found.stdout.lines().collect();
        let more = lines.len().saturating_sub(MAX_LINES);
        let shown = lines[..lines.len() - more].join("\n");
        Ok(if more > 0 { format!("{shown}\n[{more} more matches: narrow the search]") } else { shown })
    }
}

kit::export!(Search);
