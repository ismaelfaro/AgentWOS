# AGENTS.md

Guidance for coding agents working in this repository. People should start with
[README.md](README.md).

## What this is

AgentWOS is an experimental operating system for AI agents. Everything that runs
is a WebAssembly component (agents, tools, model clients, controls, scenarios).
A Rust host on Wasmtime is the kernel: it loads components, mediates every call
between them and records what happened. A second kernel in JavaScript runs the
same components in a browser.

`design/` describes where the project is going. Much of it is **not built**.
Check the code before assuming a design term exists in it.

## Layout

| Path | Contents |
|---|---|
| `wit/world.wit` | The one contract. Every interface and world. Host and guests generate bindings from it |
| `host/` | Native kernel (Rust, Wasmtime). The only member of the root cargo workspace |
| `modules/*` | Shared guest components. Each is a standalone crate, outside the root workspace |
| `web/kernel/` | Browser kernel (plain JavaScript, no dependencies) |
| `examples/*` | Self-contained examples. Each owns its components, manifest, build script and UI |
| `design/` | Design proposals: `<feature>.md`, `build/<feature>.md`, `stories/<feature>.md` |
| `tools/serve.py` | Static server that disables browser caching |

Generated and local-only, never committed: `target/`, `node_modules/`,
`examples/*/web/{gen,dist,vendor,samples,deploy}`, `examples/*/runs`, `Docs/`.

## Build and test

Guests build for `wasm32-wasip2` (`rustup target add wasm32-wasip2`). If `cargo`
is not on `PATH`, run `source "$HOME/.cargo/env"` first.

```bash
cargo build --release                      # the host
cargo test -p agentwos-host                # host unit tests
(cd modules/<name> && cargo build --release --target wasm32-wasip2)

examples/cli-harness/build.sh && examples/cli-harness/smoke.sh   # no model needed
examples/mars-colony/build.sh
examples/tab-jam/build.sh                  # needs Node; downloads the pinned Cactus engine
(cd examples/tab-jam/web && node test/smoke.mjs)                 # needs the Needle model (35 MB, fetched once)
tools/serve.py                             # then open http://localhost:8765/examples/<name>/...
```

Each example's `build.sh` builds everything that example needs. Read it before
building by hand; build order matters where the host embeds components.

Models: `local/<id>` is LM Studio on `localhost:1234`; `openrouter/<id>` needs
`OPENROUTER_API_KEY`. Do not print or commit keys.

## Rules of the architecture

- **Everything that runs is a WASM component.** New behaviour goes in a
  component with a WIT interface, not in the host. The host only loads, links,
  checks and records. If something must be host code, say why in a comment.
- **One contract.** Change `wit/world.wit`, never a copy. After a change,
  rebuild every guest that uses the touched world and the host.
- **No ambient authority.** A component gets no network, files or programs
  unless the host grants them. Do not widen a grant to make something work;
  find out what it needs.
- **One instance, one Store.** Components do not share memory. The host
  mediates.
- **Examples are self-contained.** Example-specific code, data, dashboards and
  logs live under that example's folder.
- **Keep JSON key order for tool declarations** (`serde_json` with
  `preserve_order`). The Needle model is sensitive to it.

## Conventions

- Repository text is English: code, comments, docs, commit messages.
- Comments say why, not what. One short line.
- A change to a UI is not done until it has been used in a browser. A change to
  a guest is not done until its example's smoke test passes.
- Say plainly what was not verified (audio, a real touchscreen, a model you
  could not run).
- A new feature design gets three documents: concept, build notes, stories. Use
  the vocabulary in `design/README.md`. The code still uses older names
  (`control` for guard, `grants` in manifests); do not rename code to match the
  design unless asked.

## Git

- Commit only when asked. Stage files by name; never `git add -A`. Other
  sessions often leave unrelated changes in the tree, and they are not yours to
  commit.
- Check the current branch before committing. Work may be on a feature branch.
- Use a GitHub noreply address as author and committer. Do not put a personal
  email into history, and do not change git config to do it.
- Never commit recordings, `Docs/`, scratch folders or `.DS_Store`.
- Do not push unless asked.

## Where to look first

| Task | Start at |
|---|---|
| Add a plugin agent to the coding harness | `examples/cli-harness/plugins/echo` (the one to copy), `examples/cli-harness/kit` |
| Add a control | `examples/mars-colony/controls/guardia` |
| Add a scenario or stage | `examples/mars-colony/scenario`, `examples/tab-jam/stage` |
| Change how calls are mediated | `host/src/runtime.rs`, `host/src/harness.rs`, `web/kernel/kernel.js` |
| Understand the direction | `design/README.md`, then `design/roadmap.md` |

The project skill in `.claude/skills/agentwos/` has step-by-step procedures for
these.
