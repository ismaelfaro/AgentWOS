wit_bindgen::generate!({
    world: "tool-module",
    path: "../../wit/world.wit",
});

use exports::agentmodules::core::tool::{CallError, Guest, ToolCall};
use serde_json::json;

struct Component;

impl Guest for Component {
    fn describe() -> String {
        json!({
            "name": "echo",
            "description": "Devuelve el texto recibido, en mayusculas. Ejemplo de tool minimo.",
            "json_schema": {
                "type": "object",
                "properties": { "text": { "type": "string" } },
                "required": ["text"]
            }
        })
        .to_string()
    }

    fn invoke(call: ToolCall) -> Result<String, CallError> {
        let args: serde_json::Value = serde_json::from_str(&call.args_json)
            .map_err(|e| CallError::InvalidInput(e.to_string()))?;

        let text = args
            .get("text")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CallError::InvalidInput("falta campo 'text'".into()))?;

        Ok(json!({ "result": text.to_uppercase() }).to_string())
    }
}

export!(Component);
