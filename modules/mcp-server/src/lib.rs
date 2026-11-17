//! MCP server bridge. It parses what an outside MCP client sends, answers the
//! few methods AgentWOS offers (`server/discover`, `tools/list`, `tools/call`,
//! and `initialize` for older clients), and reaches the agents only through the
//! `exposed` import, which the kernel implements with a guard on every call.
//!
//! Stateless: nothing is kept between messages.

wit_bindgen::generate!({
    world: "mcp-server-module",
    path: "../../wit/world.wit",
});

use agentwos::core::exposed;
use exports::agentwos::core::mcp_server::Guest;
use serde_json::{json, Value};

struct Component;

const CURRENT: &str = "2026-07-28";
/// Revisions this server answers for, newest first. A client of an older
/// revision is answered in its own terms.
const SUPPORTED: [&str; 3] = [CURRENT, "2025-11-25", "2025-06-18"];

fn error(id: &Value, code: i64, message: &str) -> String {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } }).to_string()
}

fn reply(id: &Value, mut result: Value, modern: bool) -> String {
    if modern {
        result["resultType"] = json!("complete");
    }
    json!({ "jsonrpc": "2.0", "id": id, "result": result }).to_string()
}

fn server_info() -> Value {
    json!({ "name": "agentwos", "version": "0.1.0" })
}

fn text_result(text: &str, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "isError": is_error })
}

fn handle_request(method: &str, params: &Value, id: &Value, modern: bool) -> String {
    match method {
        "server/discover" => reply(
            id,
            json!({ "supportedVersions": SUPPORTED, "capabilities": { "tools": {} }, "serverInfo": server_info() }),
            modern,
        ),
        // Older clients open with this.
        "initialize" => {
            let asked = params["protocolVersion"].as_str().unwrap_or("");
            let version = SUPPORTED.iter().skip(1).find(|v| **v == asked).copied().unwrap_or(SUPPORTED[1]);
            reply(id, json!({ "protocolVersion": version, "capabilities": { "tools": {} }, "serverInfo": server_info() }), false)
        }
        "ping" => reply(id, json!({}), false),
        "tools/list" => match serde_json::from_str::<Value>(&exposed::list_agents()) {
            Ok(tools) => reply(id, json!({ "tools": tools }), modern),
            Err(_) => error(id, -32603, "the list of agents could not be read"),
        },
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return error(id, -32602, "tools/call needs a `name`");
            };
            let arguments = if params["arguments"].is_null() { json!({}) } else { params["arguments"].clone() };
            match exposed::call_agent(name, &arguments.to_string()) {
                Ok(text) => reply(id, text_result(&text, false), modern),
                // An unknown agent is a protocol error; a failing one is a tool error.
                Err(agentwos::core::types::CallError::NotFound(m)) => error(id, -32602, &m),
                Err(e) => {
                    let message = match e {
                        agentwos::core::types::CallError::InvalidInput(m)
                        | agentwos::core::types::CallError::ExecutionFailed(m)
                        | agentwos::core::types::CallError::NotFound(m) => m,
                    };
                    reply(id, text_result(&message, true), modern)
                }
            }
        }
        other => error(id, -32601, &format!("method '{other}' is not offered")),
    }
}

impl Guest for Component {
    fn handle(message: String) -> Option<String> {
        let Ok(request) = serde_json::from_str::<Value>(&message) else {
            return Some(error(&Value::Null, -32700, "parse error"));
        };
        // A message without an id is a notification: no reply.
        let id = request.get("id").cloned()?;
        let Some(method) = request["method"].as_str() else {
            return Some(error(&id, -32600, "invalid request"));
        };
        let params = if request["params"].is_null() { json!({}) } else { request["params"].clone() };
        let modern = params["_meta"]["io.modelcontextprotocol/protocolVersion"].as_str().is_some();
        Some(handle_request(method, &params, &id, modern))
    }
}

export!(Component);
