#!/usr/bin/env bash
# Builds the CLI harness: the components the host embeds, the LLM client it
# shares with the other examples, the router and every plugin (one cargo
# workspace), and the native host.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"

# The host embeds these at compile time, so they come before it.
for m in mcp-guard mcp-client mcp-server a2a-client dashboard; do
  (cd "$root/modules/$m" && cargo build --release --target wasm32-wasip2)
done
(cd "$root/modules/llm-call" && cargo build --release --target wasm32-wasip2)
(cd "$here" && cargo build --release --target wasm32-wasip2)
(cd "$root" && cargo build --release)
echo "built: $root/target/release/agentwos-host"
