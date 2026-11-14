# AgentWOS CLI harness

A command-line harness where every plugin is another agent. One router agent
receives what you type; each plugin is its own WASM component in its own Store,
and the router reaches all of them through the ordinary `tool` import.

```
you ──► router.wasm ──tool──► host ──► echo.wasm
              │                        wordcount.wasm
              └──llm                   ask.wasm ───────llm
                                       workspace.wasm ──────workspace
                                       coder.wasm ─────llm + workspace
                                                        │        │
                                              llm-call.wasm   one folder,
                                              (key, network)  allowed programs
```

A plugin is a component with the `plugin-module` world: text in, text out. It
may import `llm` and `workspace`, and the manifest decides which of them each
plugin actually gets.

## Use it

```bash
rustup target add wasm32-wasip2
examples/cli-harness/build.sh
export OPENROUTER_API_KEY=sk-or-...        # only for what uses a model
./target/release/agentwos-host --harness examples/cli-harness/manifest.json
```

There are two ways to ask for something:

- **`/<plugin> <input>`** calls that plugin directly. No model is involved.
- **Anything else** goes to the model, which calls plugins one after another
  until it can answer. Each call and its result are shown.

```
> /wordcount one two three
words: 3
characters: 13
lines: 1

> create a file notes.txt containing hello harness, then list the workspace
[workspace] write notes.txt hello harness
  wrote notes.txt (13 bytes)
[workspace] list
  file       13  notes.txt
Created notes.txt with content 'hello harness'. Workspace files:
- notes.txt (13 bytes)

> /coder write primes.py that prints the primes below 30 and run it
write primes.py
  wrote primes.py (218 bytes)
run python3 primes.py
  exit 0
  [2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
Created primes.py ... it output: [2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
```

Other ways to run it:

```bash
host="./target/release/agentwos-host --harness examples/cli-harness/manifest.json"
$host --message "/echo hello"      # one request, then exit
$host --list                       # the plugins and their grants
$host --model local/google/gemma-4-e4b   # another model for this run
```

`/help` lists the plugins, `/exit` leaves the prompt.

## The plugins

| Plugin | Grants | What it does |
|---|---|---|
| `echo` | none | Returns its input. A wiring check. |
| `wordcount` | none | Counts words, characters and lines. |
| `ask` | `llm` | Answers a question with a model. `model:<id> question` picks one. |
| `workspace` | `workspace` | `write`, `read`, `list` and `run` in the workspace folder. |
| `coder` | `llm`, `workspace` | Given a task, writes code, runs it and fixes it until it works. |

`workspace` commands:

```
write <path> <text>      write a one-line file
write <path>             write a file from the lines that follow
read <path>              show a file
list [path]              list a folder
run <program> [args]     run an allowed program; quotes group words
```

File content is taken exactly as given. `run` starts the program without a
shell, so pipes and redirection are not available.

## The manifest

```json
{
  "router": "router/target/wasm32-wasip2/release/cli_router.wasm",
  "llm": "../../modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm",
  "model": "openrouter/nvidia/nemotron-3-ultra-550b-a55b:free",

  "workspace": "runs/workspace",
  "allow": ["python3", "node", "cargo", "rustc", "pytest"],
  "timeout_seconds": 30,

  "plugins": [
    { "name": "coder", "description": "...", "agent": "plugins/coder/...wasm", "grants": ["llm", "workspace"] }
  ]
}
```

Paths resolve from the manifest's folder. `model` takes an `openrouter/<id>` or
a `local/<id>` (LM Studio) model. `description` is what the model reads to
decide when to call a plugin, so write it for that reader.

## What a plugin can and cannot do

- **Nothing by default.** A plugin without grants can compute on its input and
  nothing else: no network, no files, no processes, no other plugins.
- **`llm`** gives it a model through the host's LLM client. The API key and
  `wasi:http` stay in that client; the plugin never sees them.
- **`workspace`** gives it one folder. Paths must be relative, and a path that
  leaves the folder, directly or through a symlink, is refused.
- **Programs** run only if their name is in `allow`. They get a minimal
  environment (no API keys), are stopped after `timeout_seconds`, and their
  output is cut at 256 KB.
- **An ungranted import fails.** Calling `llm` or `workspace` without the grant
  returns an error that names the missing grant.

One limit to know about: `allow` decides which programs start, not what they
do. `python3` or `node`, once allowed, can read and write whatever your user
can. For real isolation, run the harness as a low-privilege user or in a
container.

## Add a plugin

1. Copy `plugins/echo` and implement `run`:

   ```rust
   wit_bindgen::generate!({ world: "plugin-module", path: "../../../../wit/world.wit" });

   impl Guest for Component {
       fn run(input: Vec<Message>) -> Result<String, CallError> {
           let text = input.last().map(|m| m.content.as_str()).unwrap_or("");
           Ok(text.to_uppercase())
       }
   }
   ```

2. Add it to `manifest.json` with a `name`, a `description`, the path to its
   `.wasm`, and the `grants` it needs.
3. Run `build.sh`. The router finds it on its own.

To use a model, call `llm::complete(&messages, "")`; an empty model name means
the harness's model. To touch files, call `workspace::read`, `write`,
`list_files` or `exec`. `plugins/coder` does both in about a hundred lines.

## Layout

- `router/`: the router agent (`agent-module` world: imports `llm` and `tool`).
- `plugins/*/`: one crate per plugin (`plugin-module` world).
- `manifest.json`: what to load and what each plugin is granted.
- `build.sh`, `smoke.sh`: build everything; test it without a model.
- Host side: `host/src/harness.rs` (manifest, plugin registry, prompt) and
  `host/src/workspace.rs` (the folder and the programs).

## Limits

- The router and `coder` depend on the model following a JSON format. A weak
  model may answer in prose, which is then shown as the answer.
- The model takes at most 6 plugin calls per request, and `coder` 8 steps.
- OS controls are not applied to plugin calls yet.
- This runs on the native host only; the browser kernel does not load plugins.
