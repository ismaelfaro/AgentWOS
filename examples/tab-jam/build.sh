#!/usr/bin/env bash
# Builds everything Tab Jam needs and leaves a static site in web/dist.
#   1. Rust components (stage, director, shared actor) -> wasm32-wasip2
#   2. Cactus engine (cactus.js + cactus.wasm), pinned and checksum-verified
#   3. jco: components -> JS + core wasm
#   4. esbuild: kernel worker and UI bundles
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
web="$here/web"

for crate in "$root/modules/actor" "$here/stage" "$here/controls/director-agentWOS"; do
  (cd "$crate" && cargo build --release --target wasm32-wasip2)
done

# Cactus v4.0.0 wasm engine (Apache-2.0). Not committed; fetched and verified here.
cactus="https://huggingface.co/Cactus-Compute/cactus/resolve/v4.0.0/releases/4.0.0/wasm"
mkdir -p "$web/vendor"
# cactus.js is CommonJS; mark the folder so Node loads it that way.
echo '{ "type": "commonjs" }' > "$web/vendor/package.json"
fetch_verified() { # name sha256
  local file="$web/vendor/$1"
  [ -f "$file" ] || curl -fsSL -o "$file" "$cactus/$1"
  echo "$2  $file" | shasum -a 256 -c - >/dev/null || { echo "checksum mismatch: $1" >&2; rm -f "$file"; exit 1; }
}
fetch_verified cactus.js   "bb61bcf0c48712bfb9cd0ade8f0748889dfde9944ac308220f42afbc605f82bc"
fetch_verified cactus.wasm "4198b30795ded7739fe6c9e80bf26d5a844767c081574d40c8dc0487cde8b4b6"

cd "$web"
[ -d node_modules ] || npm install --silent
rm -rf gen dist
# Every build gets its own asset folder, so a browser or proxy can never pair
# files of two builds, whatever it does with query strings.
build_id="$(date +%s)"
assets="dist/$build_id"
transpile() { # wasm name
  npx jco transpile "$1" -o "gen/$2" --name "$2" --instantiation async -q
  mkdir -p "$assets/wasm/$2" && cp gen/"$2"/*.wasm "$assets/wasm/$2/"
}
transpile "$root/modules/actor/target/wasm32-wasip2/release/actor.wasm" actor
transpile "$here/stage/target/wasm32-wasip2/release/tab_jam_stage.wasm" stage
transpile "$here/controls/director-agentWOS/target/wasm32-wasip2/release/director_agentwos.wasm" director-agentWOS

# Generated code and cactus.js keep Node-only branches that never run in a browser.
npx esbuild src/worker.js src/main.js src/synth.js --bundle --format=esm --target=es2022 --outdir="$assets" \
  --external:node:* --external:fs --external:path --external:crypto --external:url --external:module \
  --log-level=warning
# The engine is pinned, so it lives outside the per-build folder and stays cached.
cp vendor/cactus.wasm dist/
cp src/style.css "$assets/"
sed "s#main.js#$build_id/main.js#; s#style.css#$build_id/style.css#" src/index.html > dist/index.html
# Optional local recordings for the sampled lead (git-ignored; see README).
if [ -d samples ]; then cp -R samples "$assets/samples"; fi
echo "built: $web/dist"
