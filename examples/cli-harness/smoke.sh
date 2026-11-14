#!/usr/bin/env bash
# Smoke test for the CLI harness. Everything here calls plugins directly, so
# it needs no model. Run build.sh first.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="$here/../../target/release/agentwos-host"
[ -x "$host" ] || { echo "missing $host; run examples/cli-harness/build.sh" >&2; exit 1; }

ws="$here/runs/workspace/smoke"
rm -rf "$ws" && mkdir -p "$ws"
failed=0
h() { "$host" --harness "$here/manifest.json" "$@" 2>&1; }

# is <label> <expected output> <message>: the reply must be exactly this.
is() {
  local got; got="$(h --message "$3")"
  if [ "$got" = "$2" ]; then echo "ok    $1"; else echo "FAIL  $1"; echo "      expected: $2"; echo "      got:      $got"; failed=1; fi
}
# has <label> <text> <message>: the reply must contain this.
has() {
  local got; got="$(h --message "$3")"
  if [[ "$got" == *"$2"* ]]; then echo "ok    $1"; else echo "FAIL  $1"; echo "      wanted: $2"; echo "      got:    $got"; failed=1; fi
}

is  "echo"                         "hello"                             "/echo hello"
is  "wordcount"                    $'words: 3\ncharacters: 13\nlines: 1' "/wordcount one two three"
has "help lists plugins"           "- coder:"                          "/help"
has "bare text goes to default"    "usage: coder"                      ""
has "unknown plugin is named"     "there is no plugin 'nope'"         "/nope hi"
if h --list | grep -q "llm+workspace"; then echo "ok    --list shows grants"; else echo "FAIL  --list shows grants"; failed=1; fi

is  "write a one-line file"        "wrote smoke/hello.py (29 bytes)"   '/workspace write smoke/hello.py print("hello from workspace")'
is  "read it back"                 'print("hello from workspace")'     "/workspace read smoke/hello.py"
is  "run it"                       $'exit 0\nhello from workspace'     "/workspace run python3 smoke/hello.py"
has "list a folder"                "hello.py"                          "/workspace list smoke"
h --message '/workspace write smoke/nl.py print("a\nb")' >/dev/null
is  "write keeps backslashes"      'print("a\nb")'                     "/workspace read smoke/nl.py"
h --message $'/workspace write smoke/two.py\nprint(1)\nprint(2)' >/dev/null
is  "multi-line write"             $'exit 0\n1\n2'                     "/workspace run python3 smoke/two.py"
is  "quotes group arguments"       $'exit 0\n2'                        '/workspace run python3 -c "print(1 + 1)"'
has "failing program reports it"   "exit 1"                            '/workspace run python3 -c "raise SystemExit(1)"'

has "program not allowed"          "'bash' is not an allowed program"  "/workspace run bash -c ls"
has "program path refused"         "bare command name"                 "/workspace run /bin/ls"
has "parent path refused"          "stay inside the workspace"         "/workspace read ../../manifest.json"
has "absolute path refused"        "stay inside the workspace"         "/workspace read /etc/hosts"
ln -s /etc "$ws/out"
has "symlink out refused (read)"   "leaves the workspace"              "/workspace read smoke/out/hosts"
has "symlink out refused (write)"  "leaves the workspace"              "/workspace write smoke/out/x hi"
rm "$ws/out"

printf 'import os\nprint(os.environ.get("OPENROUTER_API_KEY"))\n' > "$ws/env.py"
got="$(OPENROUTER_API_KEY=sk-smoke-secret h --message '/workspace run python3 smoke/env.py')"
if [[ "$got" == *"None"* && "$got" != *"sk-smoke-secret"* ]]; then echo "ok    programs do not see the API key"; else echo "FAIL  programs do not see the API key: $got"; failed=1; fi

printf 'import time\ntime.sleep(60)\n' > "$ws/slow.py"
start=$SECONDS
got="$(h --model x --message '/workspace run python3 smoke/slow.py')"
if [[ "$got" == *"was stopped after"* && $((SECONDS - start)) -lt 45 ]]; then echo "ok    a program that hangs is stopped"; else echo "FAIL  a program that hangs is stopped: $got"; failed=1; fi

got="$(printf '/echo one\n/nope\n/echo two\n/exit\n' | h)"
if [[ "$got" == *"one"* && "$got" == *"error: there is no plugin"* && "$got" == *"two"* ]]; then echo "ok    prompt keeps going after an error"; else echo "FAIL  prompt keeps going after an error: $got"; failed=1; fi

rm -rf "$ws"
[ "$failed" = 0 ] && echo "cli-harness smoke: ok" || { echo "cli-harness smoke: FAILED"; exit 1; }
