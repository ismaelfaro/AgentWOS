#!/usr/bin/env bash
# Builds Tab Swarm: the two WASM agents, their JS bindings, and the page.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"

for agent in lead worker; do
  (cd "$here/agents/$agent" && cargo build --release --target wasm32-wasip2)
done

cd "$here"
[ -d node_modules ] || npm install --silent
rm -rf gen web/dist
# Each build gets its own asset folder, so a browser never pairs files of two builds.
build_id="$(date +%s)"
assets="web/dist/$build_id"
for agent in lead worker; do
  npx jco transpile "agents/$agent/target/wasm32-wasip2/release/swarm_$agent.wasm" \
    -o "gen/$agent" --name "$agent" --instantiation async -q
done
mkdir -p "$assets/wasm/worker" && cp gen/worker/*.wasm "$assets/wasm/worker/"

# Generated code keeps Node-only branches that never run in a browser.
npx esbuild web/src/main.js --bundle --format=esm --target=es2022 --outdir="$assets" \
  --external:node:* --external:fs --external:path --external:crypto --external:url --external:module \
  --log-level=warning
cp web/src/style.css "$assets/"
sed "s#main.js#$build_id/main.js#; s#style.css#$build_id/style.css#" web/src/index.html > web/dist/index.html
echo "built: $here/web/dist"
echo "run:   node $here/local/realm.mjs"
