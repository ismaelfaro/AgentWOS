wit_bindgen::generate!({
    world: "llm-module",
    path: "wit/world.wit",
});

use exports::agentmodules::core::llm::{CallError, Guest, Message};

struct Component;

/// Stub v1: no hace HTTP real todavia (ver TODO en wit/world.wit). Devuelve
/// una respuesta determinista para validar el pipeline host<->agent<->llm.
/// Fase siguiente: `cargo component build` con `include wasi:cli/imports`
/// y POST real a OpenRouter (backend remoto) u Ollama (backend local),
/// seleccionable por el prefijo del `model` ("openrouter/..." vs "local/...").
impl Guest for Component {
    fn complete(messages: Vec<Message>, model: String) -> Result<String, CallError> {
        let last = messages
            .last()
            .ok_or_else(|| CallError::InvalidInput("sin mensajes".into()))?;

        Ok(format!(
            "[stub llm model={model}] eco de tu ultimo mensaje ({}): {}",
            last.role, last.content
        ))
    }
}

export!(Component);
