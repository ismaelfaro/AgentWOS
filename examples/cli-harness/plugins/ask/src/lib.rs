//! `ask` plugin agent: sends a question to a model and returns the answer.
//! Grant: `llm`. The API key and the network stay in the host's LLM client.

use kit::*;

struct Ask;

impl Guest for Ask {
    fn run(input: Vec<Message>) -> Reply {
        let question = text(&input);
        if question.is_empty() {
            return Ok("usage: ask <question>".into());
        }
        // An empty model name means this plugin's model from the manifest.
        llm::complete(&[message("system", "Answer directly and briefly."), message("user", question)], "")
    }
}

kit::export!(Ask);
