#!/usr/bin/env bash
# Builds what the duck pond needs: the shared agent modules, its scenario (the
# ducks' bodies) and its control (duck safety). The host is built from the repo
# root with `cargo build --release`.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
for crate in "$root/modules/agent-core" "$root/modules/tool-echo" "$root/modules/llm-call" \
             "$here/scenario" "$here/controls/duck-safety"; do
  (cd "$crate" && cargo build --release --target wasm32-wasip2)
done
