#!/usr/bin/env bash
# Both directions of MCP, through the OS's sidecar. Run examples/mcp/build.sh first.
#   CLIENT: an agent calls an outside MCP server (the colony desk).
#   SERVER: an outside MCP client calls an agent of this system (search).
set -u
cd "$(dirname "$0")"
HOST=../../target/release/agentwos-host
[ -x "$HOST" ] || { echo "build first: examples/mcp/build.sh" >&2; exit 1; }
rm -f mcp.lock
python3 server.py >/dev/null 2>&1 & trap 'kill $! 2>/dev/null; rm -f mcp.lock' EXIT
sleep 1
h() { "$HOST" --harness manifest.json "$@" 2>/dev/null; }

echo "== 1. Nothing is trusted until you approve it"
h --mcp-status | grep '^mcp colony' | tail -1
yes y | "$HOST" --harness manifest.json --mcp-approve colony 2>/dev/null | tail -2

echo; echo "== 2. CLIENT: an agent with the grant uses the outside server"
h --message '/desk list' | head -c 400; echo
h --message '/desk colony.lookup gravity'

echo; echo "== 3. An agent without `mcp:colony` has no way to reach it"
"$HOST" --harness manifest.json --message '/offline colony.lookup gravity' 2>&1 | tail -1

echo; echo "== 4. SERVER: an outside client calls our agent (search), over MCP"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"server/discover"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"search","arguments":{"input":"target"}}}' \
  | "$HOST" --harness manifest.json --mcp-serve 2>/dev/null | python3 -c '
import json,sys
for line in sys.stdin:
    m=json.loads(line); r=m.get("result",{})
    if "tools" in r: print("tools offered:", [t["name"] for t in r["tools"]])
    elif "content" in r: print(r["content"][0]["text"])
    else: print("discover:", r.get("supportedVersions"))'

echo; echo "== 5. Ask the OS how the sidecar is doing"
"$HOST" --harness manifest.json --mcp-call colony.convert --message '{"value":100,"from":"km","to":"mi"}' 2>&1 | grep -v "^mcp \(private\|x\)"
