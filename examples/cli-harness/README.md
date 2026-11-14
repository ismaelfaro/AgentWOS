# AgentWOS CLI harness

A command-line coding harness where everything is a plugin and every plugin is
a small agent. A lead agent takes your request, and expert agents (git, Python,
Rust, JavaScript, search, review) do the parts they know. Which experts take
part depends on the project you point it at.

```
you ──► router ──► coder (lead, uses a model)
                     │  reads and edits files itself
                     └─ asks ──► project   what is this project and system?
                                 search    grep
                                 git       git, but never push or discard
                                 python    python3: run, test, check
                                 rust      cargo: check, build, test, run
                                 node      node, npm
                                 review ──► git, then a model
```

Each box is its own WASM component in its own instance. None of them can touch
anything by default; the manifest grants each one exactly what it needs.

## Use it

```bash
rustup target add wasm32-wasip2
examples/cli-harness/build.sh
export OPENROUTER_API_KEY=sk-or-...        # or use a local model, see below
./target/release/agentwos-host --harness examples/cli-harness/manifest.json --project path/to/your/project
```

Then say what you want:

```
> add a function is_prime(n) in primes.py with unit tests, run them, and commit
● coder add a function is_prime(n) in primes.py with unit tests, run them, and…
  ● project
    ✓ $ uname -srm 0.0s
  ⠹ thinking gemma-4-e4b 3.2s            <- live, while the model works
  · write primes.py 14 lines
  · write test_primes.py 31 lines
  ● python test test_primes.py
    ✓ $ python3 -m unittest test_primes.py 0.1s
    │ Ran 5 tests in 0.000s
    │ OK
  ● git commit -m "Added is_prime function and unit tests"
    ✓ $ git commit -m Added is_prime function and unit tests 0.0s
    │ [main d62b8d1] Added is_prime function and unit tests
  4 agent calls · 5 model calls · 3 programs · 21.1s

Added is_prime to primes.py, created unit tests, ran them, and committed both files.
```

While it works you see what is happening: `●` an agent being asked, indented
under whoever asked it; a spinner and a clock on what is being waited for (a
model thinking, a program running); `·` a file read or written; `✓`, `!` or `✗`
on how each thing ended, with the last lines a program printed. The host draws
all of this from the calls it mediates; the agents do nothing for it. It is
shown on a terminal; `--trace` shows it when the output is piped too.

- **Plain text** goes to `coder`, which remembers the earlier tasks of the
  session. `/coder new` makes it forget them.
- **`/<plugin> <input>`** calls any plugin yourself, with no model involved:
  `/git status`, `/python test`, `/search TODO`, `/project`.
- `/help` lists the plugins, `/exit` leaves.

```bash
host="./target/release/agentwos-host --harness examples/cli-harness/manifest.json --project ."
$host --list                                 # plugins, grants, programs; x = does not fit this project
$host --message "/git log"                   # one request, then exit
$host --model local/google/gemma-4-e4b       # a local model (LM Studio) for this run
```

Without `--project`, the harness works in `runs/workspace`, an empty folder to
try things in.

## The agents

| Plugin | Grants | Programs | Offered when the project has | What it does |
|---|---|---|---|---|
| `coder` | `llm` `workspace` `plugins` | none | always | Lead. Reads, writes and edits files; asks the experts for the rest. |
| `project` | `workspace` `plugins` | `uname` | always | Reports the system, the top-level files, the experts that fit, and `AGENTS.md`. |
| `files` | `workspace` | none | always | `list`, `read`, `write`. |
| `search` | `workspace` | `grep` | always | Finds text in the project. |
| `git` | `workspace` | `git` | `.git` | Everyday git. Refuses to push or to discard work. |
| `python` | `workspace` | `python3` | `*.py`, `pyproject.toml`, `requirements.txt` | `run`, `test` (pytest or unittest), `check`. |
| `rust` | `workspace` | `cargo` | `Cargo.toml`, `*.rs` | `check`, `build`, `test`, `run`, `clippy`, `fmt`, `init`. |
| `node` | `workspace` | `node`, `npm` | `package.json`, `*.js`, `*.mjs` | `run`, `test`, `script <name>`. |
| `review` | `llm` `plugins` | none | `.git` | Gets the diff from `git` and reviews it with a model. |
| `ask` | `llm` | none | always | A general question to a model. |
| `echo` | none | none | always | Returns its input. The plugin to copy. |

Only `coder`, `review` and `ask` use a model. The experts are plain code:
each is 15 to 50 lines that know one tool and its rules.

## How it adapts to the project

Every plugin can list `when` markers in the manifest: file names (`Cargo.toml`)
or suffixes (`*.py`) to look for at the top of the project. A plugin with
markers is offered to the other agents only when one is present.

So in a Rust project `coder` is told about `rust` and `git` and never hears of
`python` or `node`. The check is made again before every step: start in an
empty folder, and the `python` expert appears the moment `coder` writes its
first `.py` file.

`coder` begins each task by asking `project`, so the model starts out knowing
the operating system, what is in the folder, and whatever the project's
`AGENTS.md` asks of agents.

`when` decides what agents are told about, not what you can call: `/git init`
works in a folder that has no repository yet.

## What a plugin can and cannot do

- **Nothing by default.** A plugin without grants computes on its input and
  nothing else: no network, no files, no processes, no other plugins.
- **`llm`** gives it a model through the host's LLM client. The API key and
  the network stay in that client; the plugin never sees them. A plugin can
  have its own `model` in the manifest, so some agents can run on a small
  local model and others on a remote one.
- **`workspace`** gives it the project folder. Paths must be relative, and a
  path that leaves the folder, directly or through a symlink, is refused.
- **`allow`** lists the programs it may start there. `git` can run `git` and
  nothing else; `coder` can run nothing. Programs start without a shell, with
  a minimal environment (no API keys), are stopped after `timeout_seconds`,
  and their output is cut at 256 KB.
- **`plugins`** lets it call the other plugins. A chain of calls cannot loop:
  a plugin that is already running further up the chain refuses.
- **An ungranted import fails** with an error that names the missing grant.

Because `coder` runs no programs, an expert's rules hold for everyone: no
agent can push, because the only way to git is the `git` expert, and it does
not push.

One limit to know about: `allow` decides which programs start, not what they
do. Code that `python3`, `cargo` or `node` runs can read and write whatever
your user can. For real isolation, run the harness as a low-privilege user or
in a container.

## The manifest

```json
{
  "router": "target/wasm32-wasip2/release/router.wasm",
  "llm": "../../modules/llm-call/target/wasm32-wasip2/release/llm_call.wasm",
  "model": "openrouter/nvidia/nemotron-3-ultra-550b-a55b:free",
  "default": "coder",
  "workspace": "runs/workspace",
  "timeout_seconds": 120,

  "plugins": [
    {
      "name": "git",
      "description": "Git expert. Input: a git command without `git`: `status`, `diff`, ...",
      "agent": "target/wasm32-wasip2/release/git.wasm",
      "grants": ["workspace"],
      "allow": ["git"],
      "when": [".git"]
    }
  ]
}
```

Paths resolve from the manifest's folder. `model` takes `openrouter/<id>` or
`local/<id>` (LM Studio), for the harness or for one plugin. `default` is the
plugin that gets text typed without a `/`; without it, that text goes to the
router's model, which picks plugins itself. `description` is all another agent
knows about a plugin, so it should say what input the plugin takes.

## Add an expert

An expert for another language, SDK or system tool is one small crate and one
manifest entry.

1. Copy `plugins/echo` to `plugins/<name>` and write its logic. `kit` holds
   the bindings and the helpers, so this is a whole plugin:

   ```rust
   //! `make` expert agent. Grant: `workspace`, allowed program: `make`.
   use kit::*;

   struct Make;

   impl Guest for Make {
       fn run(input: Vec<Message>) -> Reply {
           match command(text(&input)) {
               ("clean", _) => Err(invalid("the make expert does not clean; ask the user")),
               (_, _) => run_line("make", &[], text(&input)),
           }
       }
   }

   kit::export!(Make);
   ```

2. Add it to `manifest.json`: `name`, `description`, `agent`, and the least it
   needs of `grants`, `allow` and `when` (here `["workspace"]`, `["make"]`,
   `["Makefile"]`).
3. Run `build.sh`. `coder` hears about it in any project that has a `Makefile`.

From `kit`: `text`, `command` and `words` take the request apart; `run` and
`run_line` start an allowed program; `workspace::read`, `write` and
`list_files` reach files; `llm::complete(&messages, "")` asks the plugin's
model; `agents()` and `call(name, input)` reach the other plugins.

## Layout

- `kit/`: bindings of the `plugin-module` world and helpers, shared by plugins.
- `plugins/*/`: one crate per plugin.
- `router/`: the router agent (`agent-module` world).
- `Cargo.toml`: one cargo workspace for all of the above.
- `manifest.json`: what to load and what each plugin is granted.
- `build.sh`, `smoke.sh`: build everything; test it without a model.
- Host side: `host/src/harness.rs` (manifest, plugin registry, prompt) and
  `host/src/workspace.rs` (the folder and the programs).

## Limits

- `coder` depends on the model following a JSON format. Small local models
  manage simple tasks; larger changes need a stronger model.
- `coder` takes at most 24 rounds per task, and sees at most 12,000 characters
  of any one result.
- `edit` replaces one exact piece of text; there is no multi-file refactoring
  tool or language server.
- Nothing asks for approval: an agent acts as soon as it decides to. The
  limits are the grants above.
- Markers are looked for at the top of the project only.
- OS controls are not applied to plugin calls yet.
- This runs on the native host only; the browser kernel does not load plugins.
