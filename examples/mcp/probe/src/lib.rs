//! A plugin that does one thing: use the tools its manifest gives it. `list`
//! shows them; `<tool> <input>` calls one. It is how the tests (and anyone
//! learning) see what an agent sees of an MCP server, and nothing more.

use kit::*;

struct Probe;

impl Guest for Probe {
    fn run(input: Vec<Message>) -> Reply {
        let (first, rest) = command(text(&input));
        if first == "list" {
            return Ok(tool::describe());
        }
        tool::invoke(&ToolCall { name: first.to_string(), args_json: json!({ "input": rest }).to_string() })
    }
}

kit::export!(Probe);
