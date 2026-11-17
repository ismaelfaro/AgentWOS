//! The A2A side of serving: the Agent Card and the task methods. It is plain data
//! translation. It never runs an agent itself: the caller gives it a function that
//! does, and that function is the one that runs the guards.
//!
//! What this build answers: `SendMessage`, `GetTask`, `ListTasks`, `CancelTask`.
//! A message runs to its end inside the request, so a task is `completed`, `failed`
//! or `rejected` (a guard refused it) when it is first returned. Streaming and push
//! notifications are said not to be supported, in the card and in the errors.

use std::collections::VecDeque;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

/// Tasks kept for `GetTask` and `ListTasks`. The oldest are forgotten.
const MAX_TASKS: usize = 200;

// JSON-RPC codes of the A2A errors.
const TASK_NOT_FOUND: i64 = -32001;
const TASK_NOT_CANCELABLE: i64 = -32002;
const PUSH_NOT_SUPPORTED: i64 = -32003;
const UNSUPPORTED_OPERATION: i64 = -32004;
const CONTENT_TYPE_NOT_SUPPORTED: i64 = -32005;
const VERSION_NOT_SUPPORTED: i64 = -32009;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

pub struct Skill {
    pub id: String,
    pub name: String,
    pub description: String,
}

/// How running an agent ended.
pub enum Outcome {
    Done(String),
    /// A guard refused the call.
    Refused(String),
    Failed(String),
}

#[derive(Default)]
pub struct Tasks {
    list: VecDeque<Value>,
    made: u64,
}

impl Tasks {
    fn id(&mut self) -> String {
        self.made += 1;
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let hash = Sha256::digest(format!("{nanos}:{}", self.made));
        format!("task-{}", hash.iter().take(8).map(|b| format!("{b:02x}")).collect::<String>())
    }

    fn keep(&mut self, task: Value) {
        self.list.push_back(task);
        while self.list.len() > MAX_TASKS {
            self.list.pop_front();
        }
    }

    fn find(&self, id: &str) -> Option<&Value> {
        self.list.iter().find(|t| t["id"].as_str() == Some(id))
    }
}

/// The card of this system. Only what is exposed appears in it, and it says what
/// is not supported rather than leave a client to find out.
pub fn card(skills: &[Skill], url: &str) -> Value {
    json!({
        "name": "AgentWOS",
        "description": "An operating system for agents. These skills are agents it chooses to offer; each runs with the grants listed for exposure.",
        "version": env!("CARGO_PKG_VERSION"),
        "supportedInterfaces": [{ "url": url, "protocolBinding": "JSONRPC", "protocolVersion": "1.0" }],
        "capabilities": { "streaming": false, "pushNotifications": false, "extendedAgentCard": false },
        "securitySchemes": { "bearer": { "httpAuthSecurityScheme": { "scheme": "Bearer" } } },
        "securityRequirements": [{ "schemes": { "bearer": { "list": [] } } }],
        "defaultInputModes": ["text/plain"],
        "defaultOutputModes": ["text/plain"],
        "skills": skills.iter().map(|s| json!({
            "id": s.id, "name": s.name, "description": s.description, "tags": ["agentwos"],
        })).collect::<Vec<_>>(),
    })
}

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

fn text_of(message: &Value) -> Result<String, &'static str> {
    let parts = message["parts"].as_array().filter(|p| !p.is_empty()).ok_or("the message has no parts")?;
    let mut out = Vec::new();
    for part in parts {
        match part["text"].as_str() {
            Some(t) => out.push(t.to_string()),
            None => return Err("only text parts are accepted"),
        }
    }
    Ok(out.join("\n"))
}

fn task(id: &str, context: &str, state: &str, text: &str, as_artifact: bool) -> Value {
    let mut t = json!({ "id": id, "contextId": context, "status": { "state": state } });
    let parts = json!([{ "text": text }]);
    if as_artifact {
        t["artifacts"] = json!([{ "artifactId": format!("{id}-result"), "name": "result", "parts": parts }]);
    } else {
        t["status"]["message"] = json!({ "messageId": format!("{id}-status"), "role": "ROLE_AGENT", "parts": parts });
    }
    t
}

/// One JSON-RPC request in, one response out. `version` is the `A2A-Version` header.
pub fn handle(tasks: &mut Tasks, skills: &[Skill], version: Option<&str>, request: &Value, run: &mut dyn FnMut(&str, &str) -> Outcome) -> Value {
    let id = &request["id"];
    if let Some(v) = version.filter(|v| !v.starts_with("1.")) {
        return error(id, VERSION_NOT_SUPPORTED, &format!("version {v} is not supported; this agent speaks 1.0"));
    }
    let params = &request["params"];
    match request["method"].as_str().unwrap_or("") {
        "SendMessage" => {
            let message = &params["message"];
            let text = match text_of(message) {
                Ok(t) => t,
                Err(why) if why.starts_with("only text") => return error(id, CONTENT_TYPE_NOT_SUPPORTED, why),
                Err(why) => return error(id, INVALID_PARAMS, why),
            };
            let skill = message["metadata"]["skill"].as_str().map(str::to_string).or_else(|| (skills.len() == 1).then(|| skills[0].id.clone()));
            let Some(skill) = skill.filter(|s| skills.iter().any(|k| &k.id == s)) else {
                return error(id, INVALID_PARAMS, "name one of the skills in message.metadata.skill");
            };
            if let Some(existing) = message["taskId"].as_str() {
                return match tasks.find(existing) {
                    None => error(id, TASK_NOT_FOUND, "no such task"),
                    Some(_) => error(id, UNSUPPORTED_OPERATION, "the task is finished and takes no more messages; send a new message"),
                };
            }
            let context = message["contextId"].as_str().map(str::to_string).unwrap_or_else(|| tasks.id().replace("task-", "ctx-"));
            let task_id = tasks.id();
            let done = match run(&skill, &text) {
                Outcome::Done(text) => task(&task_id, &context, "TASK_STATE_COMPLETED", &text, true),
                Outcome::Refused(why) => task(&task_id, &context, "TASK_STATE_REJECTED", &why, false),
                Outcome::Failed(why) => task(&task_id, &context, "TASK_STATE_FAILED", &why, false),
            };
            tasks.keep(done.clone());
            json!({ "jsonrpc": "2.0", "id": id, "result": { "task": done } })
        }
        "GetTask" => match params["id"].as_str().and_then(|i| tasks.find(i)) {
            Some(t) => json!({ "jsonrpc": "2.0", "id": id, "result": t }),
            None => error(id, TASK_NOT_FOUND, "no such task"),
        },
        "ListTasks" => {
            let all: Vec<Value> = tasks.list.iter().rev().cloned().collect();
            let total = all.len();
            json!({ "jsonrpc": "2.0", "id": id, "result": { "tasks": all, "nextPageToken": "", "pageSize": total, "totalSize": total } })
        }
        "CancelTask" => match params["id"].as_str().and_then(|i| tasks.find(i)) {
            None => error(id, TASK_NOT_FOUND, "no such task"),
            Some(_) => error(id, TASK_NOT_CANCELABLE, "the task already ended"),
        },
        "SendStreamingMessage" | "SubscribeToTask" => error(id, UNSUPPORTED_OPERATION, "streaming is not supported"),
        m if m.contains("PushNotificationConfig") => error(id, PUSH_NOT_SUPPORTED, "push notifications are not supported"),
        _ => error(id, METHOD_NOT_FOUND, "method not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn skills() -> Vec<Skill> {
        vec![Skill { id: "search".into(), name: "search".into(), description: "finds text".into() }]
    }

    fn send(tasks: &mut Tasks, body: Value, run: &mut dyn FnMut(&str, &str) -> Outcome) -> Value {
        handle(tasks, &skills(), Some("1.0"), &json!({ "jsonrpc": "2.0", "id": 1, "method": "SendMessage", "params": body }), run)
    }

    #[test]
    fn a_message_becomes_a_completed_task_that_can_be_read_back() {
        let mut tasks = Tasks::default();
        let reply = send(&mut tasks, json!({ "message": { "role": "ROLE_USER", "parts": [{ "text": "target" }] } }), &mut |s, t| Outcome::Done(format!("{s}:{t}")));
        assert_eq!(reply["result"]["task"]["status"]["state"], "TASK_STATE_COMPLETED");
        assert_eq!(reply["result"]["task"]["artifacts"][0]["parts"][0]["text"], "search:target");
        let id = reply["result"]["task"]["id"].as_str().unwrap().to_string();
        let got = handle(&mut tasks, &skills(), None, &json!({ "id": 2, "method": "GetTask", "params": { "id": id } }), &mut |_, _| unreachable!());
        assert_eq!(got["result"]["id"], id.as_str());
    }

    #[test]
    fn a_guard_refusal_is_a_rejected_task_and_files_are_not_accepted() {
        let mut tasks = Tasks::default();
        let reply = send(&mut tasks, json!({ "message": { "parts": [{ "text": "x" }] } }), &mut |_, _| Outcome::Refused("blocked by guard".into()));
        assert_eq!(reply["result"]["task"]["status"]["state"], "TASK_STATE_REJECTED");
        let file = send(&mut tasks, json!({ "message": { "parts": [{ "raw": "AAAA" }] } }), &mut |_, _| unreachable!());
        assert_eq!(file["error"]["code"], CONTENT_TYPE_NOT_SUPPORTED);
    }

    #[test]
    fn versions_methods_and_finished_tasks_are_answered_with_a2a_errors() {
        let mut tasks = Tasks::default();
        let old = handle(&mut tasks, &skills(), Some("0.3"), &json!({ "id": 1, "method": "SendMessage" }), &mut |_, _| unreachable!());
        assert_eq!(old["error"]["code"], VERSION_NOT_SUPPORTED);
        let m = handle(&mut tasks, &skills(), None, &json!({ "id": 1, "method": "Nope" }), &mut |_, _| unreachable!());
        assert_eq!(m["error"]["code"], METHOD_NOT_FOUND);
        let cancel = handle(&mut tasks, &skills(), None, &json!({ "id": 1, "method": "CancelTask", "params": { "id": "task-x" } }), &mut |_, _| unreachable!());
        assert_eq!(cancel["error"]["code"], TASK_NOT_FOUND);
        let stream = handle(&mut tasks, &skills(), None, &json!({ "id": 1, "method": "SendStreamingMessage" }), &mut |_, _| unreachable!());
        assert_eq!(stream["error"]["code"], UNSUPPORTED_OPERATION);
    }
}
