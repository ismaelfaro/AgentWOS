//! A2A client bridge. It makes one remote A2A agent look like the tool server the
//! kernel already knows how to supervise: the agent's *skills* are its tools, and
//! calling a tool sends the agent a message and waits for its task to finish.
//!
//! That is why it exports the same interface as the MCP bridge. Everything the
//! kernel does for MCP, it does here unchanged: the guard, the pin on every
//! definition, the network narrowed to one origin, the token it keeps, the limits.
//!
//! What the bridge adds is the A2A part: read the Agent Card, find the JSON-RPC
//! interface (and refuse one that is not on the configured origin), send
//! `SendMessage`, follow the task with `GetTask` while it works, and turn parts into
//! text. Files are never fetched or decoded; they are named and left out.

wit_bindgen::generate!({
    world: "mcp-client-module",
    path: "../../wit/world.wit",
});

use exports::agentwos::core::mcp_client::{CallError, Guest};
use serde_json::{json, Value};
use std::time::Duration;
use wstd::http::{Body, Client, Request};

struct Component;

const VERSION: &str = "1.0";
const CARD_PATH: &str = "/.well-known/agent-card.json";
/// A task that is still working after this many checks is handed back as pending.
const MAX_POLLS: usize = 20;
const POLL_EVERY: Duration = Duration::from_secs(1);

fn failed(message: impl Into<String>) -> CallError {
    CallError::ExecutionFailed(message.into())
}

fn invalid(message: impl Into<String>) -> CallError {
    CallError::InvalidInput(message.into())
}

fn base_url(server: &str) -> Result<String, CallError> {
    let v: Value = serde_json::from_str(server).map_err(|e| invalid(format!("server is not JSON: {e}")))?;
    Ok(v["url"].as_str().ok_or_else(|| invalid("server has no url"))?.trim_end_matches('/').to_string())
}

/// `scheme://authority` of a URL, lower case.
fn origin(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("", url));
    format!("{}://{}", scheme.to_ascii_lowercase(), rest.split(['/', '?', '#']).next().unwrap_or("").to_ascii_lowercase())
}

async fn send(request: Request<Body>) -> Result<(u16, String), CallError> {
    let mut response = Client::new().send(request).await.map_err(|e| failed(format!("cannot reach the server: {e}")))?;
    let status = response.status().as_u16();
    let text = response.body_mut().str_contents().await.map_err(|e| failed(format!("reading the reply: {e}")))?.to_string();
    Ok((status, text))
}

fn with_token(builder: wstd::http::request::Builder) -> wstd::http::request::Builder {
    match std::env::var("MCP_BEARER") {
        Ok(token) => builder.header("authorization", format!("Bearer {token}")),
        Err(_) => builder,
    }
}

async fn card(base: &str) -> Result<Value, CallError> {
    let request = with_token(Request::get(format!("{base}{CARD_PATH}")).header("accept", "application/json"))
        .body(Body::empty())
        .map_err(|e| failed(e.to_string()))?;
    let (status, text) = send(request).await?;
    if status != 200 {
        return Err(failed(format!("HTTP {status}: no Agent Card at {base}{CARD_PATH}")));
    }
    serde_json::from_str(&text).map_err(|_| failed("the Agent Card is not JSON"))
}

/// Where to send JSON-RPC. The card says, and the card is outside data: an interface
/// on another origin is refused here, before the network would refuse it too.
fn endpoint(card: &Value, base: &str) -> Result<String, CallError> {
    let mut found = card["supportedInterfaces"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|i| i["protocolBinding"].as_str().is_some_and(|b| b.eq_ignore_ascii_case("JSONRPC")))
        .filter_map(|i| i["url"].as_str().map(str::to_string))
        .chain(card["url"].as_str().map(str::to_string)); // the older, single-endpoint card
    let url = found.next().ok_or_else(|| failed("the Agent Card offers no JSON-RPC interface"))?;
    if origin(&url) != origin(base) {
        return Err(failed(format!("the Agent Card points to {url}, which is not the server in the manifest")));
    }
    Ok(url)
}

async fn rpc(url: &str, method: &str, params: Value) -> Result<Value, CallError> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let request = with_token(Request::post(url).header("content-type", "application/json").header("accept", "application/json").header("a2a-version", VERSION))
        .body(Body::from_json(&body).map_err(|e| failed(e.to_string()))?)
        .map_err(|e| failed(e.to_string()))?;
    let (status, text) = send(request).await?;
    let reply: Value = serde_json::from_str(&text).map_err(|_| failed(format!("HTTP {status}: not a JSON-RPC reply")))?;
    if let Some(error) = reply.get("error") {
        return Err(failed(match error.as_object() {
            Some(_) => format!("agent error {}: {}", error["code"].as_i64().unwrap_or(0), error["message"].as_str().unwrap_or("no message")),
            None => format!("HTTP {status}: {}", error.as_str().unwrap_or("the server refused the request")),
        }));
    }
    reply.get("result").cloned().ok_or_else(|| failed(format!("HTTP {status}: not a JSON-RPC reply")))
}

fn tool_name(skill_id: &str) -> String {
    skill_id.chars().map(|c| if c.is_ascii_alphanumeric() || "_-.".contains(c) { c } else { '_' }).collect()
}

/// The skills of the card, as the tool definitions the kernel pins. The agent's
/// identity goes into every definition, so a card that changes who it is changes
/// every pin.
fn tools_of(card: &Value, url: &str) -> Vec<Value> {
    let agent = card["name"].as_str().unwrap_or("agent");
    card["skills"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|skill| {
            let id = skill["id"].as_str()?;
            let examples: Vec<&str> = skill["examples"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            let mut description = format!("{} (skill of the A2A agent {agent}). {}", skill["name"].as_str().unwrap_or(id), skill["description"].as_str().unwrap_or(""));
            if !examples.is_empty() {
                description.push_str(&format!(" Examples: {}", examples.join("; ")));
            }
            Some(json!({
                "name": tool_name(id),
                "description": description,
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "message": { "type": "string", "description": "What to ask the agent" },
                        "task_id": { "type": "string", "description": "Only to answer an agent that asked for more input" }
                    },
                    "required": ["message"]
                },
                "annotations": {
                    "a2a.agent": agent,
                    "a2a.version": card["version"],
                    "a2a.interface": url,
                    "a2a.skill": id,
                    "a2a.tags": skill["tags"],
                },
            }))
        })
        .collect()
}

/// `completed`, `input-required`, ... from any spelling of a task state.
fn state_of(task: &Value) -> String {
    let raw = task["status"]["state"].as_str().unwrap_or("unknown");
    raw.trim_start_matches("TASK_STATE_").to_ascii_lowercase().replace('_', "-")
}

/// Text from parts. A file is named and left out; its bytes are never read.
fn parts_text(parts: &Value) -> String {
    let mut out = Vec::new();
    for part in parts.as_array().into_iter().flatten() {
        if let Some(text) = part["text"].as_str() {
            out.push(text.to_string());
        } else if part.get("data").is_some_and(|d| !d.is_null()) {
            out.push(part["data"].to_string());
        } else {
            let name = part["filename"].as_str().or(part["file"]["name"].as_str()).unwrap_or("unnamed");
            let kind = part["mediaType"].as_str().or(part["file"]["mimeType"].as_str()).unwrap_or("unknown type");
            out.push(format!("[file left out: {name}, {kind}]"));
        }
    }
    out.join("\n")
}

fn task_text(task: &Value) -> String {
    let mut out = Vec::new();
    for artifact in task["artifacts"].as_array().into_iter().flatten() {
        out.push(parts_text(&artifact["parts"]));
    }
    let said = parts_text(&task["status"]["message"]["parts"]);
    if !said.is_empty() {
        out.push(said);
    }
    out.retain(|s| !s.is_empty());
    out.join("\n")
}

fn result(text: String, is_error: bool) -> String {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error, "resultType": "complete" }).to_string()
}

async fn list_tools(server: String) -> Result<String, CallError> {
    let base = base_url(&server)?;
    let card = card(&base).await?;
    let url = endpoint(&card, &base)?;
    Ok(json!({ "tools": tools_of(&card, &url), "resultType": "complete" }).to_string())
}

async fn call_tool(server: String, name: String, arguments: String) -> Result<String, CallError> {
    let base = base_url(&server)?;
    let card = card(&base).await?;
    let url = endpoint(&card, &base)?;
    let skill = tools_of(&card, &url)
        .into_iter()
        .find(|t| t["name"].as_str() == Some(name.as_str()))
        .and_then(|t| t["annotations"]["a2a.skill"].as_str().map(str::to_string))
        .ok_or_else(|| invalid(format!("the agent has no skill '{name}'")))?;
    let args: Value = serde_json::from_str(&arguments).map_err(|e| invalid(format!("arguments are not JSON: {e}")))?;
    let text = args["message"].as_str().ok_or_else(|| invalid("`message` is required"))?;

    let mut message = json!({
        "messageId": format!("agentwos-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos())),
        "role": "ROLE_USER",
        "parts": [{ "text": text }],
        "metadata": { "skill": skill },
    });
    if let Some(task_id) = args["task_id"].as_str() {
        message["taskId"] = json!(task_id);
    }
    let sent = rpc(&url, "SendMessage", json!({ "message": message, "configuration": { "returnImmediately": false } })).await?;

    // The reply is a task or a plain message, in the current shape or the older one.
    let mut task = if let Some(t) = sent.get("task") {
        t.clone()
    } else if let Some(m) = sent.get("message") {
        return Ok(result(parts_text(&m["parts"]), false));
    } else if sent.get("status").is_some() {
        sent
    } else if sent.get("parts").is_some() {
        return Ok(result(parts_text(&sent["parts"]), false));
    } else {
        return Err(failed("the agent's reply is neither a task nor a message"));
    };

    let id = task["id"].as_str().unwrap_or("").to_string();
    for _ in 0..MAX_POLLS {
        if !matches!(state_of(&task).as_str(), "submitted" | "working") {
            break;
        }
        wstd::task::sleep(POLL_EVERY.into()).await;
        task = rpc(&url, "GetTask", json!({ "id": id })).await?;
    }

    let state = state_of(&task);
    let text = task_text(&task);
    Ok(match state.as_str() {
        "completed" => result(text, false),
        "input-required" | "auth-required" => result(
            format!("[the agent needs more from you before it can go on: state {state}, task {id}. To answer, call this tool again with task_id \"{id}\".]\n{text}"),
            false,
        ),
        "submitted" | "working" => result(format!("[still {state} after {MAX_POLLS} checks, task {id}. Nothing to use yet.]"), true),
        other => result(format!("[the agent ended the task as {other}]\n{text}"), true),
    })
}

impl Guest for Component {
    fn probe(server: String) -> Result<String, CallError> {
        let base = base_url(&server)?;
        wstd::runtime::block_on(async { card(&base).await.map(|_| "current".to_string()) })
    }

    fn list_tools(server: String) -> Result<String, CallError> {
        wstd::runtime::block_on(list_tools(server))
    }

    fn call_tool(server: String, name: String, arguments: String) -> Result<String, CallError> {
        wstd::runtime::block_on(call_tool(server, name, arguments))
    }
}

export!(Component);
