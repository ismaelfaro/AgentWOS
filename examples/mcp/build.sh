#!/usr/bin/env bash
# Builds everything the MCP sidecar needs, in order. The host embeds the three MCP
# components at compile time, so they must exist before the host is built.
set -euo pipefail
cd "$(dirname "$0")/../.."
for m in mcp-guard mcp-client mcp-server a2a-client; do
  (cd modules/$m && cargo build --release --target wasm32-wasip2)
done
(cd examples/mcp/probe && cargo build --release --target wasm32-wasip2)
(cd examples/cli-harness && ./build.sh)
cargo build --release -p agentwos-host
echo "built. Run examples/mcp/smoke.sh"
