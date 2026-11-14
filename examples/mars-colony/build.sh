#!/usr/bin/env bash
# Builds everything the Mars colony example needs: its scenario and its controls.
# The host and the shared kernel modules are built from the repo root.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
for crate in scenario controls/guardia; do
  (cd "$here/$crate" && cargo build --release --target wasm32-wasip2)
done
