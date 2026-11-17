//! MCP client bridge. It speaks to one server over Streamable HTTP, in the
//! current revision (stateless, `_meta` on every request) and, when the server
//! does not know that revision, in the older initialize-first one.
//!
//! The kernel gives this component one thing, the network, and the kernel's
//! egress hooks narrow even that to the host of the one server it serves. It
//! never sees an agent, a grant or a secret other than its own bearer token,
//! which the kernel puts in its environment as `MCP_BEARER`.

wit_bindgen::generate!({
    world: "mcp-client-module",
    path: "../../wit/world.wit",
});

use exports::agentwos::core::mcp_client::{CallError, Guest};
use serde_json::{json, Value};
use wstd::http::{Body, Client, Request};

struct Component;

const CURRENT: &str = "2026-07-28";
const LEGACY: &str = "2025-06-18";
/// A server that keeps sending pages of tools is stopped here.
const MAX_PAGES: usize = 20;

fn failed(message: impl Into<String>) -> CallError {
    CallError::ExecutionFailed(message.into())
}

fn invalid(message: impl Into<String>) -> CallError {
    CallError::InvalidInput(message.into())
}

struct Server {
    url: String,
    era: Option<String>,
}

fn parse_server(server: &str) -> Result<Server, CallError> {
    let v: Value = serde_json::from_str(server).map_err(|e| invalid(format!("server is not JSON: {e}")))?;
    Ok(Server {
        url: v["url"].as_str().ok_or_else(|| invalid("server has no url"))?.to_string(),
        era: v["era"].as_str().map(str::to_string),
    })
}

struct Reply {
    status: u16,
    content_type: String,
    session: Option<String>,
    text: String,
}

async fn post(url: &str, headers: &[(&str, String)], body: &Value) -> Result<Reply, CallError> {
    let mut builder = Request::post(url)
        .header("content-type", "application/json")
        .header("accept", "application/json, text/event-stream");
    for (name, value) in headers {
        builder = builder.header(*name, value.as_str());
    }
    if let Ok(token) = std::env::var("MCP_BEARER") {
        builder = builder.header("authorization", format!("Bearer {token}"));
    }
    let request = builder
        .body(Body::from_json(body).map_err(|e| failed(e.to_string()))?)
        .map_err(|e| failed(e.to_string()))?;
    let mut response = Client::new().send(request).await.map_err(|e| failed(format!("cannot reach the server: {e}")))?;
    let status = response.status().as_u16();
    let header = |name: &str| response.headers().get(name).and_then(|v| v.to_str().ok()).map(str::to_string);
    let (content_type, session) = (header("content-type").unwrap_or_default(), header("mcp-session-id"));
    let text = response.body_mut().str_contents().await.map_err(|e| failed(format!("reading the reply: {e}")))?.to_string();
    Ok(Reply { status, content_type, session, text })
}

/// The JSON-RPC response with this id, from a JSON body or an event stream.
fn response_with_id(reply: &Reply, id: u64) -> Option<Value> {
    let matches = |v: &Value| v["id"].as_u64() == Some(id) && (v.get("result").is_some() || v.get("error").is_some());
    if reply.content_type.contains("text/event-stream") {
        let mut data = String::new();
        let mut found = None;
        for line in reply.text.lines().chain(std::iter::once("")) {
            if let Some(rest) = line.strip_prefix("data:") {
                data.push_str(rest.trim_start());
                data.push('\n');
            } else if line.trim().is_empty() && !data.is_empty() {
                if let Ok(v) = serde_json::from_str::<Value>(data.trim()) {
                    if matches(&v) {
                        found = Some(v);
                    }
                }
                data.clear();
            }
        }
        return found;
    }
    match serde_json::from_str::<Value>(&reply.text).ok()? {
        Value::Array(items) => items.into_iter().find(matches),
        v if matches(&v) => Some(v),
        _ => None,
    }
}

fn result_of(response: &Value) -> Result<Value, CallError> {
    if let Some(error) = response.get("error") {
        return Err(failed(format!(
            "server error {}: {}",
            error["code"].as_i64().unwrap_or(0),
            error["message"].as_str().unwrap_or("no message")
        )));
    }
    Ok(response["result"].clone())
}

fn client_meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": CURRENT,
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": { "name": "agentwos", "version": "0.1.0" },
    })
}

/// One request in the current revision. The reply is the raw JSON-RPC response,
/// and a reply that is not JSON-RPC at all is an error with the HTTP status.
async fn current_request(url: &str, method: &str, mut params: Value, name: Option<&str>) -> Result<Value, CallError> {
    params["_meta"] = client_meta();
    let mut headers = vec![("mcp-protocol-version", CURRENT.to_string()), ("mcp-method", method.to_string())];
    if let Some(name) = name {
        headers.push(("mcp-name", name.to_string()));
    }
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let reply = post(url, &headers, &body).await?;
    response_with_id(&reply, 1).ok_or_else(|| failed(format!("HTTP {}: not a JSON-RPC reply", reply.status)))
}

/// An older server: initialize, say we are ready, then ask. The session lasts
/// for this one call, so the bridge keeps nothing between calls.
struct Legacy {
    url: String,
    headers: Vec<(&'static str, String)>,
}

async fn legacy_start(url: &str) -> Result<Legacy, CallError> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": LEGACY, "capabilities": {}, "clientInfo": { "name": "agentwos", "version": "0.1.0" } } });
    let reply = post(url, &[], &body).await?;
    let response = response_with_id(&reply, 1).ok_or_else(|| failed(format!("HTTP {}: initialize got no JSON-RPC reply", reply.status)))?;
    let result = result_of(&response)?;
    let version = result["protocolVersion"].as_str().unwrap_or(LEGACY).to_string();
    let mut headers = vec![("mcp-protocol-version", version)];
    if let Some(session) = reply.session {
        headers.push(("mcp-session-id", session));
    }
    let initialized = json!({ "jsonrpc": "2.0", "method": "notifications/initialized" });
    post(url, &headers, &initialized).await?;
    Ok(Legacy { url: url.to_string(), headers })
}

impl Legacy {
    async fn request(&self, method: &str, params: Value) -> Result<Value, CallError> {
        let body = json!({ "jsonrpc": "2.0", "id": 2, "method": method, "params": params });
        let reply = post(&self.url, &self.headers, &body).await?;
        response_with_id(&reply, 2).ok_or_else(|| failed(format!("HTTP {}: not a JSON-RPC reply", reply.status)))
    }
}

async fn probe(server: &Server) -> Result<&'static str, CallError> {
    let response = current_request(&server.url, "server/discover", json!({}), None).await;
    match response {
        Ok(response) if response.get("result").is_some() => {
            let versions: Vec<&str> =
                response["result"]["supportedVersions"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
            if !versions.is_empty() && !versions.contains(&CURRENT) {
                return Err(failed(format!("the server speaks {} and not {CURRENT}", versions.join(", "))));
            }
            Ok("current")
        }
        // A modern server that does not like our version says which it supports.
        Ok(response) if response["error"]["data"].get("supported").is_some() => {
            Err(failed("the server does not support this protocol revision"))
        }
        // A server that does not know `server/discover` is an older one.
        Ok(_) => Ok("legacy"),
        Err(e) if matches!(&e, CallError::ExecutionFailed(m) if m.starts_with("cannot reach")) => Err(e),
        Err(_) => Ok("legacy"),
    }
}

async fn era_of(server: &Server) -> Result<&'static str, CallError> {
    match server.era.as_deref() {
        Some("current") => Ok("current"),
        Some("legacy") => Ok("legacy"),
        Some(other) => Err(invalid(format!("unknown era '{other}'"))),
        None => probe(server).await,
    }
}

async fn list_tools(server: Server) -> Result<String, CallError> {
    let era = era_of(&server).await?;
    let legacy = if era == "legacy" { Some(legacy_start(&server.url).await?) } else { None };
    let mut tools = Vec::new();
    let mut cursor: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let params = cursor.as_ref().map_or(json!({}), |c| json!({ "cursor": c }));
        let response = match &legacy {
            Some(session) => session.request("tools/list", params).await?,
            None => current_request(&server.url, "tools/list", params, None).await?,
        };
        let result = result_of(&response)?;
        tools.extend(result["tools"].as_array().cloned().unwrap_or_default());
        cursor = result["nextCursor"].as_str().map(str::to_string);
        if cursor.is_none() {
            return Ok(json!({ "tools": tools }).to_string());
        }
    }
    Err(failed(format!("the server listed more than {MAX_PAGES} pages of tools")))
}

async fn call_tool(server: Server, name: String, arguments: String) -> Result<String, CallError> {
    let arguments: Value = serde_json::from_str(&arguments).map_err(|e| invalid(format!("arguments are not JSON: {e}")))?;
    let params = json!({ "name": name, "arguments": arguments });
    let response = if era_of(&server).await? == "legacy" {
        legacy_start(&server.url).await?.request("tools/call", params).await?
    } else {
        current_request(&server.url, "tools/call", params, Some(&name)).await?
    };
    let mut result = result_of(&response)?;
    // A reply without `resultType` is a complete one.
    if result.get("resultType").is_none() {
        result["resultType"] = json!("complete");
    }
    Ok(result.to_string())
}

impl Guest for Component {
    fn probe(server: String) -> Result<String, CallError> {
        let server = parse_server(&server)?;
        wstd::runtime::block_on(probe(&server)).map(str::to_string)
    }

    fn list_tools(server: String) -> Result<String, CallError> {
        wstd::runtime::block_on(list_tools(parse_server(&server)?))
    }

    fn call_tool(server: String, name: String, arguments: String) -> Result<String, CallError> {
        wstd::runtime::block_on(call_tool(parse_server(&server)?, name, arguments))
    }
}

export!(Component);
