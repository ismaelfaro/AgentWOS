# AgentWOS CLI harness

A command-line coding assistant where everything is a plugin and every plugin
is a small agent. You type a request. A lead agent reads and edits your project
and asks expert agents (git, Python, Rust, JavaScript, search, review) for the
rest. Which experts take part depends on the project you point it at.

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
                                 coder     more coders, in parallel
```

Each box is its own WASM component in its own sandbox. None of them can touch
anything by default; the manifest grants each one exactly what it needs.

Contents: [Run it](#run-it) · [The prompt](#the-prompt) ·
[What you see](#what-you-see-while-it-works) · [The agents](#the-agents) ·
[Adapting to the project](#how-it-adapts-to-the-project) ·
[The right expert](#getting-the-right-expert) · [In parallel](#agents-in-parallel) ·
[Models](#models) · [Grants](#what-a-plugin-can-and-cannot-do) ·
[Manifest](#the-manifest) · [Add an expert](#add-an-expert) · [Limits](#limits)

## Run it

From the root of the repository:

```bash
rustup target add wasm32-wasip2
examples/cli-harness/build.sh
```

With a local model in [LM Studio](https://lmstudio.ai) (server on, model loaded):

```bash
./target/release/agentwos-host --harness examples/cli-harness/manifest.json --project path/to/your/project --model local/google/gemma-4-e4b
```

With [OpenRouter](https://openrouter.ai), using the free model the manifest names:

```bash
export OPENROUTER_API_KEY=sk-or-...
./target/release/agentwos-host --harness examples/cli-harness/manifest.json --project path/to/your/project
```

Without `--project`, the harness works in `runs/workspace`, an empty folder to
try things in.

**Nothing asks for approval.** The agents write files, run code and commit as
soon as they decide to. Try it first on a folder you can throw away, or on a
repository with everything committed.

Options, all with `--harness <manifest>`:

| Option | Does |
|---|---|
| `--project <dir>` | The folder the agents work in |
| `--model <id>` | The model for this run: `local/<id>` or `openrouter/<id>` |
| `--message "<text>"` | One request, then exit |
| `--list` | The plugins, their grants and programs; `x` marks the ones that do not fit this project |
| `--trace` | Every line of every agent, with no board for parallel agents. Also shows activity when the output is piped |

## The prompt

- **Plain text** goes to `coder`. It remembers the earlier requests of the
  session; `/coder new` makes it forget them.
- **`/<plugin> <input>`** calls any plugin yourself, with no model involved:
  `/git status`, `/python test`, `/search TODO`, `/project`, `/review`.
- `/help` lists the plugins. `/exit` leaves.

The prompt stays at the bottom of the screen while agents work. You can write
the next request and press Enter before the current one has finished: it joins
a queue, shown above the prompt, and runs in its turn.

```
> /python test
● python test
│  ⠋ $ python3 -m pytest -q 1.9s
  queued  add a docstring to every function
  queued  /review
> then commit everyth▏
```

| Key | Does |
|---|---|
| Up, Down | Earlier and later requests of this session |
| Left, Right, Home, End | Move in the line (also Ctrl-B, Ctrl-F, Ctrl-A, Ctrl-E) |
| Backspace, Delete | Remove a character |
| Ctrl-U, Ctrl-K, Ctrl-W | Remove to the start, to the end, the word before |
| Ctrl-D on an empty line | Leave, like `/exit` |
| Ctrl-C | Stop the harness, also in the middle of a request |

When the input is not a terminal (a pipe, a script) the harness reads plain
lines, one request each.

## What you see while it works

```
> add a function is_prime(n) in primes.py with unit tests, run them, and commit
● coder add a function is_prime(n) in primes.py with unit tests, run them, and…
│  ● project
│  │  ✓ $ uname -srm 0.0s
│  │    ┊ Darwin 27.2.0 arm64
│  ⠹ thinking gemma-4-e4b 3.2s            <- live, while the model works
│  · write primes.py 14 lines
│  · write test_primes.py 31 lines
│  ● python test test_primes.py
│  │  ✓ $ python3 -m unittest test_primes.py 0.1s
│  │    ┊ Ran 5 tests in 0.000s
│  │    ┊ OK
│  ● git commit -m "Added is_prime function and unit tests"
│  │  ✓ $ git commit -m Added is_prime function and unit tests 0.0s
│  │    ┊ [main d62b8d1] Added is_prime function and unit tests
└─ 4 agent calls · 5 model calls · 3 programs · 21.1s

Added is_prime to primes.py, created unit tests, ran them, and committed both files.
```

| Mark | Means |
|---|---|
| `●` | An agent is asked. What it does is drawn under it |
| Spinner and clock | What is being waited for: a model thinking, a program running |
| `·` | A file read or written |
| `✓` `!` `✗` | How it ended: well, a program with a non-zero exit, an error |
| `┊` | The last lines a program printed |
| `⇉` | Several agents start at the same time |
| `└─` | What the request took |

The host draws all of this from the calls it mediates. The agents do nothing
for it, so a plugin you add shows up the same way.

## The dashboard

Add `--dashboard 127.0.0.1:8830` and a dashboard agent watches the others in a
browser: a card per agent (running, idle or in alert), a graph of who asks whom,
alerts for an agent that is stuck or failing, and a live feed.

```bash
target/release/agentwos-host --harness examples/cli-harness/manifest.json --dashboard 127.0.0.1:8830
```

The host prints a link with a one-time token; open it. The dashboard keeps
running after a `--message` answer until Ctrl-C, and works with `--serve-http`
too. It is an agent that can only read the kernel's record, so it needs no
change to the agents it watches. Design: [../../design/dashboard.md](../../design/dashboard.md).

## The agents

| Plugin | Grants | Programs | Offered when the project has | What it does |
|---|---|---|---|---|
| `coder` | `llm` `workspace` `plugins` | none | always | Lead. Reads, writes and edits files; asks the experts for the rest, one at a time or several at once. |
| `project` | `workspace` `plugins` | `uname` | always | Reports the system, the top-level files, the experts that fit, and `AGENTS.md`. |
| `files` | `workspace` | none | always | `list`, `read`, `write`. |
| `search` | `workspace` | `grep` | always | Finds text in the project. |
| `git` | `workspace` | `git` | `.git` | `status`, `diff`, `log`, `show`, `add`, `commit`, `branch`, `switch`, `stash`, `init`. Refuses to push or to discard work. |
| `python` | `workspace` | `python3` | `*.py`, `pyproject.toml`, `requirements.txt` | `run`, `test` (pytest or unittest, whichever is installed), `check`. |
| `rust` | `workspace` | `cargo` | `Cargo.toml`, `*.rs` | `check`, `build`, `test`, `run`, `clippy`, `fmt`, `init`. |
| `node` | `workspace` | `node`, `npm` | `package.json`, `*.js`, `*.mjs` | `run`, `test`, `script <name>`. |
| `review` | `llm` `plugins` | none | `.git` | Gets the diff from `git` and reviews it with a model. |
| `ask` | `llm` | none | always | A general question to a model. It knows nothing about the project. |
| `echo` | none | none | always | Returns its input. The plugin to copy. |

Only `coder`, `review` and `ask` use a model. The experts are plain code: each
is 15 to 50 lines that know one tool and its rules.

What `coder` can do in one step: read a file (or list a folder), write a file,
replace one exact piece of text in a file, ask an agent, ask several agents in
parallel, or say it is done.

## How it adapts to the project

Every plugin can list `when` markers in the manifest: file names at the top of
the project (`Cargo.toml`) or suffixes of files up to three folders down
(`*.py`). A plugin with markers is offered to the other agents only when one is
present.

So in a Rust project `coder` is told about `rust` and `git` and never hears of
`python` or `node`. The check is made again before every step: start in an
empty folder, and the `python` expert appears the moment `coder` writes its
first `.py` file. `coder` is told when that happens.

`coder` begins each task by asking `project`, so the model starts out knowing
the operating system, what is in the folder, and whatever the project's
`AGENTS.md` asks of agents.

`when` decides what agents are told about, not what you can call: `/git init`
works in a folder that has no repository yet.

## Getting the right expert

Three things steer work to the expert that should do it, none of them left to
the model's memory:

- Only the experts that fit the project are described to it.
- When `coder` writes or edits a file, the result names the expert for that
  kind of file: "To run or check examples/fib.py, ask the expert agent `python`."
- If `coder` says it is done while code it changed was never run or checked,
  it is sent back once, with the expert to ask.

Which expert goes with which file comes from the same `when` markers: an
expert with `*.py` is the expert for `.py` files.

## Agents in parallel

When a task has independent pieces, `coder` asks several agents at once: more
`coder` agents, one per piece, or several experts.

```
> write 3 small python examples in parallel (sorting, fibonacci, word count) and run each
● coder write 3 small python examples in parallel (sorting, fibonacci, word co…
│  ✓ thinking nemotron 4.7s
│  ⇉ 3 agents in parallel
│  ├ 1 ✓ coder Create a Python file 'sorting.py'…    7 model calls · 1 file · 2 programs 9.6s
│  ├ 2 ⠧ coder Create a Python file 'fibonacci.py'…  thinking nemotron 3.1s
│  ├ 3 ⠧ coder Create a Python file 'word_count.py'… $ python3 word_count.py 0.4s
```

Agents working at the same time are a board: one row each, redrawn in place,
saying what that agent is doing right now. A row that has finished says what it
took, and the board stays on screen as those lines once all have answered. The
board has no room for the detail of each agent; `--trace` gives every line of
every agent instead, each marked with its number.

Each call goes to its own instance of the agent: the host keeps a pool per
plugin and starts another instance when all are busy, so five `coder` agents
are five separate sandboxes with nothing shared but the project folder. A chain
of agents asking agents stops at four deep.

How much faster this is depends on the model server. A remote API answers the
requests side by side; a local server answers as many at once as it is set to,
and queues the rest.

## Models

A model name says where it runs: `local/<id>` is LM Studio on
`localhost:1234`, `openrouter/<id>` is OpenRouter with `OPENROUTER_API_KEY`.

Each agent that uses a model can have its own, so some can run on a small
local model and others on a remote one:

```json
{
  "model": "local/google/gemma-4-e4b",
  "plugins": [
    { "name": "coder",  "model": "openrouter/nvidia/nemotron-3-ultra-550b-a55b:free", "...": "..." },
    { "name": "review", "model": "local/qwen/qwen3.6-35b-a3b", "...": "..." },
    { "name": "ask", "...": "..." }
  ]
}
```

Which model an agent uses, first that applies:

1. The one its own code names in `llm::complete(&messages, "<model>")`. The
   plugins here pass `""`, which means "mine".
2. Its `model` in the manifest.
3. `--model` on the command line.
4. The manifest's top-level `model`.

So `--model` changes every agent that has no `model` of its own. The activity
shows the model of each call (`thinking gemma-4-e4b`).

Parallel `coder` workers are the same plugin as the lead, so they use the same
model. For a strong lead and cheap workers, add a second entry (say `worker`)
with the same `agent` file and a smaller `model`.

`coder` needs the strongest model you have: it must plan and reply in exact
JSON. `review` and `ask` do well on smaller ones.

## What a plugin can and cannot do

- **Nothing by default.** A plugin without grants computes on its input and
  nothing else: no network, no files, no processes, no other plugins.
- **`llm`** gives it a model through the host's LLM client. The API key and
  the network stay in that client; the plugin never sees them.
- **`workspace`** gives it the project folder. Paths must be relative, and a
  path that leaves the folder, directly or through a symlink, is refused.
- **`allow`** lists the programs it may start there. `git` can run `git` and
  nothing else; `coder` can run nothing. Programs start without a shell, with
  a minimal environment (no API keys), are stopped after `timeout_seconds`,
  and their output is cut at 256 KB.
- **`plugins`** lets it call the other plugins, one at a time or several at
  once.
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

| Key | Means |
|---|---|
| `router` | The router agent: sends `/<plugin>` to that plugin and everything else to `default` |
| `llm` | The LLM client component, the only one with network and the API key |
| `model` | The model of every agent that has none of its own |
| `default` | The plugin that gets text typed without a `/`. Without it, that text goes to the router's model, which picks plugins itself |
| `workspace` | The project folder when `--project` is not given |
| `timeout_seconds` | How long a program may run before it is stopped. 60 if absent |
| `plugins[].name` | What it is called: `/name`, and how agents ask for it |
| `plugins[].description` | All another agent knows about it, so it should say what input it takes |
| `plugins[].agent` | Its `.wasm` file |
| `plugins[].grants` | Any of `llm`, `workspace`, `plugins` |
| `plugins[].allow` | The programs it may run |
| `plugins[].when` | The markers of the projects it is an expert for |
| `plugins[].model` | Its own model |

Paths resolve from the manifest's folder.

## Add an expert

An expert for another language, SDK or system tool is one small crate and one
manifest entry.

1. Copy `plugins/echo` to `plugins/<name>`, set the package `name` in its
   `Cargo.toml`, and write its logic. `kit` holds the bindings and the helpers,
   so this is a whole plugin:

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

2. Add it to `manifest.json`:

   ```json
   {
     "name": "make",
     "description": "Make expert. Input: a target of the Makefile, or nothing for the default one.",
     "agent": "target/wasm32-wasip2/release/make.wasm",
     "grants": ["workspace"],
     "allow": ["make"],
     "when": ["Makefile"]
   }
   ```

3. Run `build.sh`. `coder` hears about it in any project that has a `Makefile`,
   and it shows in the activity like every other agent.

What `kit` gives a plugin:

| For | Use |
|---|---|
| Taking the request apart | `text(&input)`, `command(text)`, `words(line)` |
| Running an allowed program | `run(program, &args)`, `run_line(program, &leading, line)` |
| Files of the project | `workspace::read`, `workspace::write`, `workspace::list_files` |
| A model | `llm::complete(&messages, "")` |
| Other agents | `agents()`, `call(name, input)`, `call_all(&asks)` for several at once |
| Replies of a model | `json_objects(reply)`, `clip(text, max)` |
| Errors | `invalid(message)`, `text_of(error)` |

## Layout

- `kit/`: bindings of the `plugin-module` world and helpers, shared by plugins.
- `plugins/*/`: one crate per plugin.
- `router/`: the router agent (`agent-module` world).
- `Cargo.toml`: one cargo workspace for all of the above.
- `manifest.json`: what to load and what each plugin is granted.
- `build.sh`: builds the components the host embeds (MCP, A2A and dashboard), the LLM client, the guests and the host.
- `smoke.sh`: 32 checks that call plugins directly, so they need no model.
- `dashboard-smoke.sh`: starts the harness with a dashboard on a free port and reads its API: token and `Host` checks, running state, failures, who asked whom. No model.
- Host side, in `host/src/`: `harness.rs` (manifest, plugin pool, calls
  between agents), `workspace.rs` (the folder and the programs), `activity.rs`
  (what is shown), `ledger.rs` (what is kept for the dashboard), `dashboard.rs` (serving it), `prompt.rs` (the line editor and the queue).

## Limits

- **No approval step.** An agent acts as soon as it decides to. The limits are
  the grants.
- **The model has to follow a JSON format.** Small local models manage simple
  tasks and sometimes take detours; larger changes need a stronger model.
- `coder` takes at most 24 rounds per task, and sees at most 12,000 characters
  of any one result.
- `edit` replaces one exact piece of text; there is no multi-file refactoring
  tool or language server.
- Agents working at the same time are not kept from writing the same file; the
  last write wins.
- There is no time limit on a model call: a busy local server makes an agent
  wait, and the clock on the spinner is how you notice.
- A request that is running cannot be stopped without stopping the harness, and
  a request in the queue cannot be removed.
- The history of requests lasts for the session; it is not saved.
- What `coder` does wrong on its own (a reply that is not JSON, an `edit` that
  matches nothing) is not shown; you see it think again.
- Which model suits which agent is chosen by hand.
- OS controls are not applied to plugin calls yet.
- This runs on the native host only; the browser kernel does not load plugins.
