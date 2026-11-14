//! `review` expert agent: reviews the changes not yet committed.
//! Grants: `llm`, `plugins`. It has no access to the project itself: it asks
//! the `git` agent for the diff and a model for the review.

use kit::*;

struct Review;

const DIFF_CHARS: usize = 24_000;
const SYSTEM: &str = "\
You review a code change given as a git diff. List real problems first (bugs, broken edge cases, \
security), each with file and line and a concrete fix. Then at most three improvements. Be brief. \
If the change is fine, say so in one line.";

/// A diff with nothing in it is just the `exit 0` line of the git agent.
fn diff(args: &str) -> Result<Option<String>, CallError> {
    let out = call("git", args)?;
    Ok(out.split_once('\n').map(|(_, body)| body.to_string()).filter(|body| !body.trim().is_empty()))
}

impl Guest for Review {
    fn run(input: Vec<Message>) -> Reply {
        let Some(diff) = diff("diff").transpose().or_else(|| diff("diff --staged").transpose()).transpose()? else {
            return Ok("nothing to review: no uncommitted changes to tracked files".into());
        };
        let focus = match text(&input) {
            "" => String::new(),
            focus => format!("Pay special attention to: {focus}\n\n"),
        };
        llm::complete(&[message("system", SYSTEM), message("user", format!("{focus}{}", clip(&diff, DIFF_CHARS)))], "")
    }
}

kit::export!(Review);
