#!/usr/bin/env bash
# Builds the match simulator (src/sim.rs) as a plain WASM module for dashboard/lab.html.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
(cd "$here/lab" && cargo build --release --target wasm32-unknown-unknown)
cp "$here/lab/target/wasm32-unknown-unknown/release/duck_soccer_lab.wasm" "$here/dashboard/lab.wasm"
