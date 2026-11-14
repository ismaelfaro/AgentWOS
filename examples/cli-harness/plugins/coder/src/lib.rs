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
/// How much of a step's result is shown on screen, and given to the model.
const SHOWN_CHARS: usize = 500;
const RESULT_CHARS: usize = 12_000;
/// Earlier tasks of this session the model is reminded of.
const REMEMBERED_MESSAGES: usize = 8;

/// The session so far: each task and how it ended. A plugin instance lives as
/// long as the harness, so this is the agent's memory between requests.
static SESSION: Mutex<Vec<Message>> = Mutex::new(Vec::new());

/// The other agents that fit the project right now, one "- name: description" each.
fn experts() -> Vec<String> {
    let others = agents().into_iter().filter(|(name, _)| name != "coder");
    others.map(|(name, description)| format!("- {name}: {description}")).collect()
}

fn system(experts: &[String]) -> String {
    format!(
        "You are a coding agent working in a project folder. You act only through JSON steps, one object per step:\n\
         {{\"read\": \"<path>\"}}\n\
         {{\"write\": {{\"path\": \"<path>\", \"content\": \"<whole file>\"}}}}\n\
         {{\"edit\": {{\"path\": \"<path>\", \"old\": \"<exact text now in the file>\", \"new\": \"<its replacement>\"}}}}\n\
         {{\"ask\": {{\"agent\": \"<name>\", \"input\": \"<request>\"}}}}\n\
         {{\"done\": \"<what you did and what the result was>\"}}\n\
         Paths are relative to the project. You cannot run programs: ask the expert agent that owns the tool. \
         Expert agents for this project (more appear as the project grows):\n{}\n\
         Read a file before you edit it. Run or test what you wrote before you finish. \
         Send `done` alone, after you have seen the results of your steps. Reply with JSON only.",
        experts.join("\n")
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

/// Applies one step. Returns a label for the screen and the result.
fn apply(step: &Value) -> Result<(String, String), String> {
    if let Some(path) = step["read"].as_str() {
        return Ok((format!("read {path}"), workspace::read(path).map_err(text_of)?));
    }
    if let Some(write) = step.get("write") {
        let path = field(write, "path")?;
        return Ok((format!("write {path}"), workspace::write(path, field(write, "content")?).map_err(text_of)?));
    }
    if let Some(change) = step.get("edit") {
        let path = field(change, "path")?;
        return Ok((format!("edit {path}"), edit(path, field(change, "old")?, field(change, "new")?)?));
    }
    if let Some(ask) = step.get("ask") {
        let (agent, input) = (field(ask, "agent")?, ask["input"].as_str().unwrap_or(""));
        let label = format!("{agent}: {}", input.lines().next().unwrap_or(""));
        return Ok((label, call(agent, input).map_err(text_of)?));
    }
    if step.is_null() {
        return Err("the reply was not valid JSON. Inside a JSON string write a backslash as \\\\ and a quote as \\\"".into());
    }
    Err(format!("a step must be one of read, write, edit, ask, done; got {}", clip(&step.to_string(), 120)))
}

fn show(label: &str, result: &str) {
    println!("* {label}\n    {}", clip(result.trim_end(), SHOWN_CHARS).replace('\n', "\n    "));
}

fn work(task: &str) -> Reply {
    // The `project` agent, when there is one, says what environment this is.
    let project = call("project", "").map(|report| format!("{report}\n\n")).unwrap_or_default();
    let mut known = experts();
    let mut messages = vec![message("system", system(&known))];
    messages.extend(SESSION.lock().unwrap().iter().cloned());
    messages.push(message("user", format!("{project}Task: {task}")));

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
                if i == 0 {
                    return Ok(done.to_string());
                }
                break;
            }
            let applied = apply(step);
            let failed = applied.is_err();
            let (label, result) = applied.unwrap_or_else(|problem| ("error".to_string(), problem));
            show(&label, &result);
            results += &format!("{label}:\n{}\n", clip(&result, RESULT_CHARS));
            if failed {
                break; // the steps after it assumed this one worked
            }
        }
        // The project may have changed which experts fit: say so when it did.
        let now = experts();
        let arrived: Vec<&String> = now.iter().filter(|e| !known.contains(e)).collect();
        if !arrived.is_empty() {
            let list = arrived.iter().map(|e| e.as_str()).collect::<Vec<_>>().join("\n");
            results += &format!("Expert agents now available for this project:\n{list}\n");
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
