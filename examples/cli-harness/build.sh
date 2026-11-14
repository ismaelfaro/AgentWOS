#!/usr/bin/env bash
# Builds the CLI harness: the LLM client it shares with the other examples,
# the router and every plugin (one cargo workspace), and the native host.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"

(cd "$root/modules/llm-call" && cargo build --release --target wasm32-wasip2)
(cd "$here" && cargo build --release --target wasm32-wasip2)
(cd "$root" && cargo build --release)
echo "built: $root/target/release/agentwos-host"
