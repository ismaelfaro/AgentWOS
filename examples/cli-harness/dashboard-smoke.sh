#!/usr/bin/env bash
# Smoke test for the dashboard agent. It starts the harness with a dashboard on a
# free loopback port, makes the agents work, and reads the dashboard's API. It
# needs no model. Run build.sh first.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="$here/../../target/release/agentwos-host"
[ -x "$host" ] || { echo "missing $host; run examples/cli-harness/build.sh" >&2; exit 1; }

ws="$here/runs/dashboard-smoke"
rm -rf "$ws" && mkdir -p "$ws"
err="$(mktemp)"
failed=0
ok() { if [ "$1" = 0 ]; then echo "ok    $2"; else echo "FAIL  $2"; [ -n "${3:-}" ] && echo "      got: $3"; failed=1; fi; }

# The harness reads requests from stdin and exits when they end, so keep stdin
# open while the checks run. The python request sleeps, to be seen running.
{
  echo '/echo hello'
  echo '/files write a.txt hi'
  echo '/python run -c "raise SystemExit(1)"'
  echo '/nope x'
  echo '/python run -c "import time; time.sleep(6)"'
  sleep 40
} | "$host" --harness "$here/manifest.json" --project "$ws" --dashboard 127.0.0.1:0 >/dev/null 2>"$err" &
pid=$!
trap 'kill $pid 2>/dev/null; rm -f "$err"' EXIT

url=""
for _ in $(seq 50); do url="$(grep -o 'http://[^ ]*' "$err" | head -1)"; [ -n "$url" ] && break; sleep 0.1; done
[ -n "$url" ] || { echo "FAIL  the host printed no dashboard link"; cat "$err"; exit 1; }
base="${url%%/#*}"
token="${url##*token=}"
api() { curl -s -H "Authorization: Bearer $token" "$base/$1"; }
# field <json> <python expression over d>
field() { python3 -c "import json,sys; d=json.load(sys.stdin); print($2)" <<<"$1"; }

got="$(curl -s -o /dev/null -w '%{http_code} %{content_type}' "$base/")"
[[ "$got" == "200 text/html"* ]]; ok $? "the page needs no token" "$got"
got="$(curl -s -o /dev/null -w '%{http_code}' "$base/api/snapshot")"
[ "$got" = 401 ]; ok $? "the data needs the token" "$got"
got="$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer $token" -H 'Host: evil.example' "$base/api/snapshot")"
[ "$got" = 403 ]; ok $? "a foreign Host name is refused" "$got"
got="$(curl -s -o /dev/null -w '%{http_code}' -H "Authorization: Bearer wrong" "$base/api/snapshot")"
[ "$got" = 401 ]; ok $? "a wrong token is refused" "$got"

# While the python request sleeps, python is running and the router is too.
running=0
for _ in $(seq 40); do
  snap="$(api 'api/snapshot?after=0')"
  running="$(field "$snap" "[a['busy'] for a in d['agents'] if a['name']=='python'][0]" 2>/dev/null)"
  [ "${running:-0}" -ge 1 ] 2>/dev/null && break
  sleep 0.15
done
[ "${running:-0}" -ge 1 ] 2>/dev/null; ok $? "python shows as running while it works" "$running"
got="$(field "$snap" "d['totals']['busy']")"
[ "$got" -ge 1 ] 2>/dev/null; ok $? "the totals count what is running" "$got"
got="$(field "$snap" "[a['state'] for a in d['agents'] if a['name']=='python'][0]")"
[ "$got" = alert ]; ok $? "an agent that has failed more than it worked is an alert, running or not" "$got"

# After it finishes: counters and events.
for _ in $(seq 60); do
  snap="$(api 'api/snapshot?after=0')"
  [ "$(field "$snap" "d['totals']['busy']")" = 0 ] && break
  sleep 0.25
done
got="$(field "$snap" "[a['calls'] for a in d['agents'] if a['name']=='echo']")"
[ "$got" = "[1]" ]; ok $? "echo was called once" "$got"
got="$(field "$snap" "[a['files'] for a in d['agents'] if a['name']=='files']")"
[ "$got" = "[1]" ]; ok $? "the file write is counted for files" "$got"
got="$(field "$snap" "[a['errors'] for a in d['agents'] if a['name']=='python']")"
[ "$got" = "[1]" ]; ok $? "a program that exits 1 counts as a failure for python" "$got"
got="$(field "$snap" "[a['errors'] for a in d['agents'] if a['name']=='router']")"
[ "$got" = "[1]" ]; ok $? "the unknown plugin counts as a failed request" "$got"
got="$(field "$snap" "sorted({e['kind'] for e in d['events']})")"
[ "$got" = "['end', 'file', 'program', 'start']" ]; ok $? "events cover start, end, file and program" "$got"
got="$(field "$snap" "[e['caller'] for e in d['events'] if e['agent']=='echo' and e['kind']=='start']")"
[ "$got" = "['router']" ]; ok $? "an event says who asked" "$got"

got="$(field "$snap" "sorted((e['from'], e['to'], e['calls']) for e in d['edges'] if e['to'] in ('echo','router'))")"
[ "$got" = "[('router', 'echo', 1), ('you', 'router', 5)]" ]; ok $? "the graph has who asked whom, with counts" "$got"

# Reading again from the last event brings nothing already seen.
next="$(field "$snap" "d['next']")"
got="$(field "$(api "api/snapshot?after=$next")" "len(d['events'])")"
[ "$got" = 0 ]; ok $? "asking after the last event returns none" "$got"

got="$(api 'api/report?agent=python')"
[[ "$got" == *"python:"* && "$got" == *"program"* ]]; ok $? "the report names the agent and what it ran" "$got"
got="$(api 'api/report?agent=nobody')"
[[ "$got" == *"no agent 'nobody' has run"* ]]; ok $? "the report says when an agent is unknown" "$got"

# Not on loopback: refused before anything is served.
got="$("$host" --harness "$here/manifest.json" --project "$ws" --dashboard 0.0.0.0:0 --message '/echo x' 2>&1)"
[[ "$got" == *"not a loopback address"* ]]; ok $? "a non-loopback address is refused" "$got"

[ "$failed" = 0 ] && echo "all checks passed" || { echo "some checks failed"; exit 1; }
