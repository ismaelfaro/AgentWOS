wit_bindgen::generate!({
    world: "llm-module",
    path: "../../wit/world.wit",
});

use exports::agentmodules::core::llm::{CallError, Guest, Message};
use serde_json::{json, Value};
use wstd::http::{Body, Client, Request};

struct Component;

const OPENROUTER_URL: &str = "https://openrouter.ai/api/v1/chat/completions";
const LM_STUDIO_URL: &str = "http://localhost:1234/v1/chat/completions";

enum Backend {
    OpenRouter,
    Local,
}

/// The model prefix selects the backend: "openrouter/<id>" or "local/<id>".
fn route(model: &str) -> Result<(Backend, &str), CallError> {
    if let Some(id) = model.strip_prefix("openrouter/") {
        Ok((Backend::OpenRouter, id))
    } else if let Some(id) = model.strip_prefix("local/") {
        Ok((Backend::Local, id))
    } else {
        Err(CallError::InvalidInput(format!(
            "model '{model}' must start with 'openrouter/' or 'local/'"
        )))
    }
}

async fn post_chat(url: &str, api_key: Option<String>, body: Value) -> Result<String, String> {
    let mut builder = Request::post(url).header("content-type", "application/json");
    if let Some(key) = api_key {
        builder = builder.header("authorization", format!("Bearer {key}"));
    }
    let req = builder
        .body(Body::from_json(&body).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let mut resp = Client::new().send(req).await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let text = resp
        .body_mut()
        .str_contents()
        .await
        .map_err(|e| e.to_string())?
        .to_string();

    if !status.is_success() {
        return Err(format!("HTTP {status}: {text}"));
    }

    let parsed: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    parsed["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("response has no choices[0].message.content: {text}"))
}

/// The API key comes from the host environment (WasiCtx), never hardcoded in the wasm.
async fn complete_async(messages: Vec<Message>, model: String) -> Result<String, CallError> {
    let (backend, model_id) = route(&model)?;

    let msgs: Vec<Value> = messages
        .iter()
        .map(|m| json!({ "role": m.role, "content": m.content }))
        .collect();
    let body = json!({ "model": model_id, "messages": msgs });

    let result = match backend {
        Backend::OpenRouter => {
            let key = std::env::var("OPENROUTER_API_KEY").map_err(|_| {
                CallError::InvalidInput("OPENROUTER_API_KEY missing from the host environment".into())
            })?;
            post_chat(OPENROUTER_URL, Some(key), body).await
        }
        Backend::Local => post_chat(LM_STUDIO_URL, None, body).await,
    };

    result.map_err(CallError::ExecutionFailed)
}

impl Guest for Component {
    fn complete(messages: Vec<Message>, model: String) -> Result<String, CallError> {
        if messages.is_empty() {
            return Err(CallError::InvalidInput("no messages".into()));
        }
        wstd::runtime::block_on(complete_async(messages, model))
    }
}

export!(Component);
