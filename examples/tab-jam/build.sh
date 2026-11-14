#!/usr/bin/env bash
# Builds everything Tab Jam needs and leaves a static site in web/dist.
#   1. Rust components (stage, sound engineer, shared actor) -> wasm32-wasip2
#   2. Cactus engine (cactus.js + cactus.wasm), pinned and checksum-verified
#   3. jco: components -> JS + core wasm
#   4. esbuild: kernel worker and UI bundles
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
root="$(cd "$here/../.." && pwd)"
web="$here/web"

for crate in "$root/modules/actor" "$here/stage" "$here/controls/sound-engineer"; do
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
transpile() { # wasm name
  npx jco transpile "$1" -o "gen/$2" --name "$2" --instantiation async -q
  mkdir -p "dist/wasm/$2" && cp gen/"$2"/*.wasm "dist/wasm/$2/"
}
transpile "$root/modules/actor/target/wasm32-wasip2/release/actor.wasm" actor
transpile "$here/stage/target/wasm32-wasip2/release/tab_jam_stage.wasm" stage
transpile "$here/controls/sound-engineer/target/wasm32-wasip2/release/sound_engineer.wasm" sound-engineer

build_id="$(date +%s)"
# Generated code and cactus.js keep Node-only branches that never run in a browser.
npx esbuild src/worker.js src/main.js src/synth.js --bundle --format=esm --target=es2022 --outdir=dist \
  --external:node:* --external:fs --external:path --external:crypto --external:url --external:module \
  --define:__BUILD_ID__="\"$build_id\"" --log-level=warning
cp vendor/cactus.wasm dist/
# The build id in every asset URL keeps browsers from mixing files of two builds.
sed "s/main.js/main.js?v=$build_id/; s/style.css/style.css?v=$build_id/" src/index.html > dist/index.html
cp src/style.css dist/
echo "built: $web/dist"
