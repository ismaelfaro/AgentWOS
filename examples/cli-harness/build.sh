#!/usr/bin/env bash
# Builds the CLI harness: the LLM client it shares with the other examples,
# the router, every plugin, and the native host.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"

for crate in "$root/modules/llm-call" "$here/router" "$here"/plugins/*/; do
  (cd "$crate" && cargo build --release --target wasm32-wasip2)
done
(cd "$root" && cargo build --release)
echo "built: $root/target/release/agentwos-host"
