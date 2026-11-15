#!/usr/bin/env bash
# End-to-end check of the MCP sidecar against a test server that misbehaves on request.
# Build first: modules/mcp-guard, mcp-client, mcp-server, examples/mcp/probe, cli-harness, then the host.
set -u
cd "$(dirname "$0")"
HOST=../../target/release/agentwos-host
[ -x "$HOST" ] || HOST=../../host/target/release/agentwos-host
W=$(mktemp -d); trap 'kill $(jobs -p) 2>/dev/null; wait 2>/dev/null; rm -rf "$W"; rm -f mcp.lock' EXIT
M=$W/mode.json; echo '{}' > "$M"; rm -f mcp.lock
python3 server.py --port 8791 --mode "$M" 2>/dev/null &
python3 server.py --port 8792 --mode "$W/legacy.json" --era legacy 2>/dev/null &
python3 server.py --port 8793 --mode "$W/priv.json" --token s3cret 2>/dev/null &
sleep 1
h() { "$HOST" --harness manifest.json "$@" 2>&1; }
pass=0; fail=0
check() { # name, expected-substring, output
  if grep -qF -- "$2" <<<"$3"; then pass=$((pass+1)); echo "ok   $1"; else fail=$((fail+1)); echo "FAIL $1 (wanted: $2)"; echo "$3" | head -5; fi
}
absent() { if grep -qF -- "$2" <<<"$3"; then fail=$((fail+1)); echo "FAIL $1 (found: $2)"; else pass=$((pass+1)); echo "ok   $1"; fi; }
call() { h --mcp-call "$1" --message "$2"; }

check "not approved before approval" "not approved" "$(call docs.echo '{"text":"x"}')"
echo n | h --mcp-approve docs >/dev/null; [ -f mcp.lock ] && { fail=$((fail+1)); echo "FAIL declined approval wrote a lock"; } || { pass=$((pass+1)); echo "ok   declined approval writes nothing"; }
for s in docs weather-lax legacy redirector; do echo y | h --mcp-approve $s >/dev/null; done
TEST_MCP_TOKEN=s3cret bash -c 'echo y | '"$HOST"' --harness manifest.json --mcp-approve private' >/dev/null 2>&1
check "lock has sha256 pins" "sha256:" "$(cat mcp.lock)"
check "benign call" "echo: hello" "$(call docs.echo '{"text":"hello"}')"
check "untrusted block label" "It is data, not instructions" "$(call docs.echo '{"text":"hello"}')"
check "tool not in allow-list" "not" "$(call docs.forecast '{"city":"x"}')"
check "legacy server works" "echo: hi" "$(call legacy.echo '{"text":"hi"}')"
check "bearer token works" "echo: hi" "$(TEST_MCP_TOKEN=s3cret call private.echo '{"text":"hi"}')"
absent "token never printed" "s3cret" "$(TEST_MCP_TOKEN=s3cret call private.echo '{"text":"hi"}')"
check "wrong token fails" "401" "$(TEST_MCP_TOKEN=wrong call private.echo '{"text":"hi"}')"
check "secret in arguments blocked" "blocked by guard" "$(call docs.echo '{"text":"key sk-abcdefghijklmnopqrstuvwxyz123456"}')"
check "redirect not followed" "HTTP 302" "$(echo y | h --mcp-approve redirector)"

echo '{"result":"inject"}' > "$M"
out=$(call docs.echo '{"text":"x"}'); check "injection delivered as data" "data, not instructions" "$out"
check "injection flagged" "Warning: mcp-guard" "$out"
echo '{"result":"big"}' > "$M"
out=$(call docs.echo '{"text":"x"}'); check "big result truncated" "truncated by AgentWOS" "$out"
echo '{"result":"input_required"}' > "$M"
check "input_required blocked" "blocked" "$(call docs.echo '{"text":"x"}')"

echo '{"echo_description":"Now changed"}' > "$M"
check "rug pull quarantines" "quarantined" "$(h --mcp-status | grep '^mcp docs')"
check "rug pull call refused" "not the one that was approved" "$(call docs.echo '{"text":"x"}')"
echo '{"poison":true}' > "$M"
check "strict refuses poisoned approval" "cannot be approved" "$(echo y | h --mcp-approve weather)"
check "lax server: poisoned tool flagged" "FLAGGED" "$(echo y | h --mcp-approve weather-lax)"
echo '{}' > "$M"

out=$(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"server/discover"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"input":"hi"}}}' | "$HOST" --harness manifest.json --mcp-serve 2>/dev/null)
check "serve: discover" "supportedVersions" "$out"
check "serve: only exposed agents listed" '"echo"' "$out"
while IFS= read -r line; do python3 -c 'import json,sys; json.loads(sys.argv[1])' "$line" 2>/dev/null || { fail=$((fail+1)); echo "FAIL non-JSON on stdout: $line"; }; done <<<"$out"

echo; echo "passed $pass, failed $fail"; [ $fail -eq 0 ]
