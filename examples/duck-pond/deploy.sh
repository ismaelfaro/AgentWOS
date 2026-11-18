#!/usr/bin/env bash
# Builds the duck pond web page and publishes it to Cloudflare with Wrangler.
#   examples/duck-pond/deploy.sh [worker-name]     (default: agentic-duck-pond)
# Needs a Wrangler login: `npx wrangler login`.
# The Needle model is served by the Worker (worker/edge.js), so it is not uploaded.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
name="${1:-agentic-duck-pond}"

"$here/build-web.sh"

out="$here/web/deploy"
rm -rf "$out"
mkdir -p "$out"
rsync -a --exclude model "$here/web/dist/" "$out/"

cat > "$out/_headers" <<'HEADERS'
/
  Cache-Control: no-cache
/index.html
  Cache-Control: no-cache
/*
  X-Content-Type-Options: nosniff
HEADERS

cp "$here/worker/edge.js" "$out/edge.js"
cat > "$out/wrangler.jsonc" <<JSON
{
  "name": "$name",
  "main": "edge.js",
  "compatibility_date": "2026-11-13",
  "assets": {
    "directory": "./",
    "binding": "ASSETS",
    "run_worker_first": ["/model/*"]
  }
}
JSON

cd "$out"
npx --yes wrangler deploy --config wrangler.jsonc
