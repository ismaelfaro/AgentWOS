//! The MCP guard and the two MCP bridges are compiled into the kernel, so that
//! an MCP connection can never exist without them, and so that the host is one
//! file. They have to be built before the host:
//!
//!     for m in mcp-guard mcp-client mcp-server a2a-client; do
//!       (cd modules/$m && cargo build --release --target wasm32-wasip2); done

use std::path::Path;

fn main() {
    for (dir, file) in [("mcp-guard", "mcp_guard"), ("mcp-client", "mcp_client"), ("mcp-server", "mcp_server"), ("a2a-client", "a2a_client")] {
        let path = format!("../modules/{dir}/target/wasm32-wasip2/release/{file}.wasm");
        println!("cargo:rerun-if-changed={path}");
        if !Path::new(&path).exists() {
            panic!(
                "a component compiled into the kernel is missing: {path}\n\
                 build it first:  (cd modules/{dir} && cargo build --release --target wasm32-wasip2)"
            );
        }
    }
}
