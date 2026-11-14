#!/usr/bin/env bash
# Smoke test for the CLI harness. Everything here calls plugins directly, so
# it needs no model. Run build.sh first.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
host="$here/../../target/release/agentwos-host"
[ -x "$host" ] || { echo "missing $host; run examples/cli-harness/build.sh" >&2; exit 1; }

ws="$here/runs/smoke"
rm -rf "$ws" && mkdir -p "$ws"
failed=0
h() { "$host" --harness "$here/manifest.json" --project "$ws" "$@" 2>&1; }
ok() { if [ "$1" = 0 ]; then echo "ok    $2"; else echo "FAIL  $2"; [ -n "${3:-}" ] && echo "      got: $3"; failed=1; fi; }

# is <label> <expected output> <message>: the reply must be exactly this.
is() { local got; got="$(h --message "$3")"; [ "$got" = "$2" ]; ok $? "$1" "$got"; }
# has <label> <text> <message>: the reply must contain this.
has() { local got; got="$(h --message "$3")"; [[ "$got" == *"$2"* ]]; ok $? "$1" "$got"; }
# hasnt <label> <text> <message>: the reply must not contain this.
hasnt() { local got; got="$(h --message "$3")"; [[ "$got" != *"$2"* ]]; ok $? "$1" "$got"; }

is  "echo"                          "hello"                             "/echo hello"
has "help lists plugins"            "- coder:"                          "/help"
has "bare text goes to default"     "usage: coder"                      ""
has "unknown plugin is named"       "there is no plugin 'nope'"         "/nope hi"
got="$(h --list)"; [[ "$got" == *"llm+workspace+plugins"* ]]; ok $? "--list shows grants" "$got"

# Experts are offered to agents only in a project that has something for them.
hasnt "empty project: no python"    "python"                            "/project"
is  "write a one-line file"         "wrote hello.py (25 bytes)"         '/files write hello.py print("hello from files")'
has "project now fits python"       "python"                            "/project"
hasnt "but still not rust"          "rust"                              "/project"
has "project reports the system"    "System: "                          "/project"

is  "read it back"                  'print("hello from files")'         "/files read hello.py"
is  "python runs it"                $'exit 0\nhello from files'         "/python run hello.py"
has "list a folder"                 "hello.py"                          "/files list"
h --message '/files write nl.py print("a\nb")' >/dev/null
is  "write keeps backslashes"       'print("a\nb")'                     "/files read nl.py"
h --message $'/files write two.py\nprint(1)\nprint(2)' >/dev/null
is  "multi-line write"              $'exit 0\n1\n2'                     "/python run two.py"
is  "quotes group arguments"        $'exit 0\n2'                        '/python run -c "print(1 + 1)"'
has "failing program reports it"    "exit 1"                            '/python run -c "raise SystemExit(1)"'
h --message '/files write bad.py print(' >/dev/null
has "python checks syntax"          "SyntaxError"                       '/python check bad.py'
has "search finds text"             "hello.py:1:"                       "/search hello"
has "search finds nothing"          "no matches"                        "/search zzzzqq"

has "git init"                      "Initialized"                       "/git init"
has "git status"                    "hello.py"                          "/git status"
has "git refuses to push"           "does not run 'push'"               "/git push origin main"
has "git refuses to discard"        "does not run 'reset'"              "/git reset --hard"
has "review asks git, finds none"   "nothing to review"                 "/review"

# Each plugin runs only its own programs, and only inside the project.
has "python cannot be asked for git" "has no 'git' command"             "/python git status"
has "parent path refused"           "stay inside the workspace"         "/files read ../../manifest.json"
has "absolute path refused"         "stay inside the workspace"         "/files read /etc/hosts"
ln -s /etc "$ws/out"
has "symlink out refused (read)"    "leaves the workspace"              "/files read out/hosts"
has "symlink out refused (write)"   "leaves the workspace"              "/files write out/x hi"
rm "$ws/out"

printf 'import os\nprint(os.environ.get("OPENROUTER_API_KEY"))\n' > "$ws/env.py"
got="$(OPENROUTER_API_KEY=sk-smoke-secret h --message '/python run env.py')"
[[ "$got" == *"None"* && "$got" != *"sk-smoke-secret"* ]]; ok $? "programs do not see the API key" "$got"

got="$(printf '/echo one\n/nope\n/echo two\n/exit\n' | h)"
[[ "$got" == *"one"* && "$got" == *"error: there is no plugin"* && "$got" == *"two"* ]]; ok $? "prompt keeps going after an error" "$got"

rm -rf "$ws"
[ "$failed" = 0 ] && echo "cli-harness smoke: ok" || { echo "cli-harness smoke: FAILED"; exit 1; }
