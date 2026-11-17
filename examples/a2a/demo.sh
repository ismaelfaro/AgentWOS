#!/usr/bin/env bash
# One AgentWOS serves an agent over A2A; another uses it as a peer. Run examples/mcp/build.sh first.
set -u
cd "$(dirname "$0")"
HOST=../../target/release/agentwos-host
[ -x "$HOST" ] || { echo "build first: examples/mcp/build.sh" >&2; exit 1; }
export AGENTWOS_SERVE_TOKEN=local-demo-token-0123456789
rm -f mcp.lock; trap 'kill $(jobs -p) 2>/dev/null; wait 2>/dev/null; rm -f mcp.lock' EXIT
"$HOST" --harness serve.json --serve-http 127.0.0.1:8820 2>/dev/null &
sleep 2
T="Authorization: Bearer $AGENTWOS_SERVE_TOKEN"

echo "== 1. The serving side publishes an Agent Card (only what it exposes)"
curl -s -H "$T" http://127.0.0.1:8820/.well-known/agent-card.json | python3 -c '
import json,sys; c=json.load(sys.stdin)
print(c["name"], c["version"], "| interface:", c["supportedInterfaces"][0]["url"], "| streaming:", c["capabilities"]["streaming"])
for s in c["skills"]: print("  skill:", s["id"], "-", s["description"])'

echo; echo "== 2. No token, no answer"
curl -s -i http://127.0.0.1:8820/.well-known/agent-card.json | head -1

echo; echo "== 3. The other AgentWOS approves the peer: it shows the pinned definition"
echo y | "$HOST" --harness manifest.json --mcp-approve peer 2>/dev/null | grep -E "offers|a2a.agent|Approved"

echo; echo "== 4. An agent there calls the peer's skill, through the sidecar (A2A: task, artifact)"
"$HOST" --harness manifest.json --mcp-call peer.search --message '{"message":"Reserve"}' 2>/dev/null | grep -v "^mcp "

echo; echo "== 5. An outside client tries to read outside the notes"
curl -s -H "$T" -H "A2A-Version: 1.0" -d '{"jsonrpc":"2.0","id":1,"method":"SendMessage","params":{"message":{"parts":[{"text":"root /etc/passwd"}],"metadata":{"skill":"search"}}}}' http://127.0.0.1:8820/a2a | python3 -c '
import json,sys; t=json.load(sys.stdin)["result"]["task"]; print(t["status"]["state"], "-", t["status"]["message"]["parts"][0]["text"])'
