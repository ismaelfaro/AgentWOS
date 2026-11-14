//! `coder` plugin agent: the lead. Given a task, it reads and edits the
//! project's files and asks the expert agents for everything else, until the
//! task is done.
//! Grants: `llm`, `workspace`, `plugins`. It may run no program itself: the
//! expert that owns a tool runs it, under that expert's own rules.
//!
//! The model proposes steps as JSON and this agent applies them. Which experts
//! it is told about depends on the project, and is looked up again before
//! every step, so an expert appears once the project has something for it.

use std::sync::Mutex;

use kit::*;

struct Coder;

/// Steps the model may take for one task.
const MAX_STEPS: usize = 24;
/// How much of a step's result is given to the model.
const RESULT_CHARS: usize = 12_000;
/// Earlier tasks of this session the model is reminded of.
const REMEMBERED_MESSAGES: usize = 8;

/// The session so far: each task and how it ended. A plugin instance lives as
/// long as the harness, so this is the agent's memory between requests.
static SESSION: Mutex<Vec<Message>> = Mutex::new(Vec::new());

/// The other agents that fit the project right now.
fn experts() -> Vec<Agent> {
    agents().into_iter().filter(|agent| agent.name != "coder").collect()
}

fn listed(experts: &[Agent]) -> String {
    experts.iter().map(|e| format!("- {}: {}", e.name, e.description)).collect::<Vec<_>>().join("\n")
}

fn system(experts: &[Agent]) -> String {
    format!(
        "You are a coding agent working in a project folder. You act only through JSON steps, one object per step:\n\
         {{\"read\": \"<path>\"}}\n\
         {{\"write\": {{\"path\": \"<path>\", \"content\": \"<whole file>\"}}}}\n\
         {{\"edit\": {{\"path\": \"<path>\", \"old\": \"<exact text now in the file>\", \"new\": \"<its replacement>\"}}}}\n\
         {{\"ask\": {{\"agent\": \"<name>\", \"input\": \"<request>\"}}}}\n\
         {{\"parallel\": [{{\"agent\": \"<name>\", \"input\": \"<request>\"}}, {{\"agent\": \"<name>\", \"input\": \"<request>\"}}]}}\n\
         {{\"done\": \"<what you did and what the result was>\"}}\n\
         Paths are relative to the project. You cannot run programs: ask the expert agent that owns the tool. \
         Expert agents for this project (more appear as the project grows):\n{}\n\
         - coder: another coding agent like you, for `parallel`. Give it one self-contained piece of work: \
         which file to create and what it must do.\n\
         `parallel` asks several agents at the same time. Use it when the task has independent pieces \
         (several files, several examples, several checks): one entry per piece.\n\
         Read a file before you edit it. After you write or change code, ask the expert for that language \
         to run or test it. Send `done` alone, after you have seen the results of your steps. Reply with JSON only.",
        listed(experts)
    )
}

fn field<'a>(step: &'a Value, key: &str) -> Result<&'a str, String> {
    step[key].as_str().ok_or_else(|| format!("this step needs a `{key}`"))
}

/// Replaces the one place `old` occurs in a file.
fn edit(path: &str, old: &str, new: &str) -> Result<String, String> {
    let before = workspace::read(path).map_err(text_of)?;
    match before.matches(old).count() {
        1 => workspace::write(path, &before.replacen(old, new, 1)).map_err(text_of),
        0 => Err(format!("`old` does not occur in {path}; read the file and copy the text exactly")),
        n => Err(format!("`old` occurs {n} times in {path}; include more of the surrounding text")),
    }
}

/// A file's text, or what is in the folder when the path is one.
fn read(path: &str) -> Result<String, String> {
    workspace::read(path).or_else(|problem| match workspace::list_files(path) {
        Ok(entries) => Ok(format!(
            "{path} is a folder containing: {}",
            entries.iter().map(|e| e.path.as_str()).collect::<Vec<_>>().join("  ")
        )),
        Err(_) => Err(text_of(problem)),
    })
}

/// Says which expert runs or checks a file that was just changed. Asked
/// afresh, because the change may be what makes that expert fit the project.
fn whose(path: &str) -> String {
    let owners: Vec<String> = experts().iter().filter(|e| e.knows(path)).map(|e| format!("`{}`", e.name)).collect();
    if owners.is_empty() {
        return String::new();
    }
    format!("\nTo run or check {path}, ask the expert agent {}.", owners.join(" or "))
}

/// What one task has done so far that still matters.
#[derive(Default)]
struct Progress {
    /// Files written or edited that no expert for them has been asked about since.
    unchecked: Vec<String>,
    /// Whether the model was already told to have them checked.
    reminded: bool,
}

impl Progress {
    fn changed(&mut self, path: &str) {
        if !self.unchecked.iter().any(|p| p == path) {
            self.unchecked.push(path.to_string());
        }
    }

    /// Asking an expert settles the files it is the expert for.
    fn asked(&mut self, agent: &str, experts: &[Agent]) {
        if let Some(expert) = experts.iter().find(|e| e.name == agent) {
            self.unchecked.retain(|path| !expert.knows(path));
        }
    }

    /// The experts for files that were changed and never run or checked.
    fn owed<'a>(&self, experts: &'a [Agent]) -> Vec<&'a Agent> {
        experts.iter().filter(|e| self.unchecked.iter().any(|path| e.knows(path))).collect()
    }
}

/// Applies one step. Returns a label and the result, both for the model.
fn apply(step: &Value, progress: &mut Progress, experts: &[Agent]) -> Result<(String, String), String> {
    if let Some(path) = step["read"].as_str() {
        return Ok((format!("read {path}"), read(path)?));
    }
    if let Some(write) = step.get("write") {
        let path = field(write, "path")?;
        let result = workspace::write(path, field(write, "content")?).map_err(text_of)?;
        progress.changed(path);
        return Ok((format!("write {path}"), result + &whose(path)));
    }
    if let Some(change) = step.get("edit") {
        let path = field(change, "path")?;
        let result = edit(path, field(change, "old")?, field(change, "new")?)?;
        progress.changed(path);
        return Ok((format!("edit {path}"), result + &whose(path)));
    }
    if let Some(ask) = step.get("ask") {
        let (agent, input) = (field(ask, "agent")?, ask["input"].as_str().unwrap_or(""));
        progress.asked(agent, experts);
        return Ok((format!("{agent}: {}", input.lines().next().unwrap_or("")), call(agent, input).map_err(text_of)?));
    }
    if let Some(asks) = step["parallel"].as_array() {
        let asks: Vec<(String, String)> = asks
            .iter()
            .map(|ask| Ok((field(ask, "agent")?.to_string(), ask["input"].as_str().unwrap_or("").to_string())))
            .collect::<Result<_, String>>()?;
        let mut report = String::new();
        for ((agent, input), answer) in asks.iter().zip(call_all(&asks)) {
            progress.asked(agent, experts);
            let answer = answer.unwrap_or_else(|e| format!("error: {}", text_of(e)));
            report += &format!("[{agent}: {}]\n{answer}\n", input.lines().next().unwrap_or(""));
        }
        return Ok((format!("parallel, {} agents", asks.len()), report));
    }
    if step.is_null() {
        return Err("the reply was not valid JSON. Inside a JSON string write a backslash as \\\\ and a quote as \\\"".into());
    }
    Err(format!("a step must be one of read, write, edit, ask, parallel, done; got {}", clip(&step.to_string(), 120)))
}

fn work(task: &str) -> Reply {
    // The `project` agent, when there is one, says what environment this is.
    let project = call("project", "").map(|report| format!("{report}\n\n")).unwrap_or_default();
    let mut known = experts();
    let mut messages = vec![message("system", system(&known))];
    messages.extend(SESSION.lock().unwrap().iter().cloned());
    messages.push(message("user", format!("{project}Task: {task}")));
    let mut progress = Progress::default();

    for _ in 0..MAX_STEPS {
        let reply = llm::complete(&messages, "")?;
        let mut steps = json_objects(&reply);
        if steps.is_empty() {
            if !reply.contains('{') {
                return Ok(reply.trim().to_string()); // plain words are the answer
            }
            steps.push(Value::Null); // JSON that does not parse: tell the model
        }
        let mut results = String::new();
        for (i, step) in steps.iter().enumerate() {
            if let Some(done) = step["done"].as_str() {
                // `done` counts only on its own: sent together with other
                // steps, the model has not seen their results yet.
                if i > 0 {
                    break;
                }
                // Changed code that nobody ran is not done: name who should.
                let owed = progress.owed(&known);
                if owed.is_empty() || progress.reminded {
                    return Ok(done.to_string());
                }
                progress.reminded = true;
                results += &format!(
                    "Not done yet: you changed {} and no expert has run or checked it. Ask:\n{}\nThen send `done`.\n",
                    progress.unchecked.join(", "),
                    owed.iter().map(|e| format!("- {}: {}", e.name, e.description)).collect::<Vec<_>>().join("\n")
                );
                break;
            }
            let applied = apply(step, &mut progress, &known);
            let failed = applied.is_err();
            let (label, result) = applied.unwrap_or_else(|problem| ("error".to_string(), problem));
            results += &format!("{label}:\n{}\n", clip(&result, RESULT_CHARS));
            if failed {
                break; // the steps after it assumed this one worked
            }
        }
        // The project may have changed which experts fit: say so when it did.
        let now = experts();
        let arrived: Vec<Agent> = now.iter().filter(|e| !known.contains(e)).cloned().collect();
        if !arrived.is_empty() {
            results += &format!("Expert agents now available for this project:\n{}\n", listed(&arrived));
        }
        messages[0] = message("system", system(&now));
        known = now;
        messages.push(message("assistant", reply));
        messages.push(message("user", format!("Results:\n{results}")));
    }
    Ok(format!("Stopped after {MAX_STEPS} rounds without finishing."))
}

impl Guest for Coder {
    fn run(input: Vec<Message>) -> Reply {
        match text(&input) {
            "" => Ok("usage: coder <what to build, fix or change>   (coder new: forget the session)".into()),
            "new" => {
                SESSION.lock().unwrap().clear();
                Ok("session forgotten".into())
            }
            task => {
                let outcome = work(task)?;
                let mut session = SESSION.lock().unwrap();
                session.extend([message("user", format!("Task: {task}")), message("assistant", outcome.clone())]);
                let extra = session.len().saturating_sub(REMEMBERED_MESSAGES);
                session.drain(..extra);
                Ok(outcome)
            }
        }
    }
}

kit::export!(Coder);
