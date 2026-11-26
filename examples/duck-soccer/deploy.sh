#!/usr/bin/env bash
# Runs both lineages and publishes the duck soccer dashboard to Cloudflare with Wrangler.
#   examples/duck-soccer/deploy.sh [worker-name]     (default: agentic-duck-soccer)
# Needs a Wrangler login: `npx wrangler login`.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
name="${1:-agentic-duck-soccer}"

cd "$here"
cargo run --release -q -- --pop 8 --gens 10 --eval 100 --seed 1
cargo run --release -q -- --pop 8 --gens 10 --eval 100 --seed 1 --talk 0 --out runs/silent.js

out="$here/web/deploy"
rm -rf "$out"
mkdir -p "$out/runs"
# The page sits at the root, so its logs move from ../runs to runs.
sed 's#\.\./runs/#runs/#g' dashboard/index.html > "$out/index.html"
cp runs/latest.js runs/silent.js "$out/runs/"

cat > "$out/_headers" <<'HEADERS'
/*
  Cache-Control: no-cache
  X-Content-Type-Options: nosniff
HEADERS

printf '.wrangler\nwrangler.jsonc\n' > "$out/.assetsignore"
cat > "$out/wrangler.jsonc" <<JSON
{
  "name": "$name",
  "compatibility_date": "2026-11-13",
  "assets": { "directory": "./" }
}
JSON

cd "$out"
npx --yes wrangler deploy --config wrangler.jsonc
