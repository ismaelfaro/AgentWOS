#!/usr/bin/env bash
# Builds Tab Jam and publishes it to Cloudflare Pages with Wrangler.
#   examples/tab-jam/deploy.sh [worker-name]     (default: agentic-tab-jam)
# Publishes a Worker that serves the static site (Workers static assets).
# Needs a Wrangler login: `npx wrangler login`.
#
# Local recordings in web/samples are never uploaded: the site is deployed from
# a copy of the build without them, so visitors get the synthesized lead.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
name="${1:-agentic-tab-jam}"

"$here/build.sh"

out="$here/web/deploy"
rm -rf "$out"
mkdir -p "$out"
rsync -a --exclude samples "$here/web/dist/" "$out/"

# Each build lives in its own folder, so only the entry page must be revalidated.
cat > "$out/_headers" <<'HEADERS'
/
  Cache-Control: no-cache
/index.html
  Cache-Control: no-cache
/*
  X-Content-Type-Options: nosniff
HEADERS

# The edge Worker serves the Needle model (see worker/edge.js); everything else
# is a static asset. Requests for the model run the Worker first.
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
