# Components

Every component is a Rust `cdylib` crate built for `wasm32-wasip2`, with
bindings generated from `wit/world.wit`.

## Which world

| You are adding | World | Copy from |
|---|---|---|
| A plugin agent for the coding harness | `plugin-module` (through `kit`) | `examples/cli-harness/plugins/echo` |
| A tool | `tool-module` | `modules/tool-echo` |
| A control | `control-module` | `examples/mars-colony/controls/guardia` |
| A turn-based scenario | `scenario-module` | `examples/mars-colony/scenario` |
| An interactive stage | `stage-module` | `examples/tab-jam/stage` |
| An agent that acts only through tool calls | `actor-module` | `modules/actor` |

Check `wit/world.wit` for the current list; worlds are added over time.

## A plugin agent for the coding harness

The harness guests are one cargo workspace (`examples/cli-harness/Cargo.toml`,
members `kit`, `router`, `plugins/*`), so a new folder under `plugins/` is
picked up automatically.

1. Copy `plugins/echo` to `plugins/<name>`; set `name` in its `Cargo.toml`.
2. Write the logic in `src/lib.rs`: implement `Guest::run`, end with
   `kit::export!(<Type>);`. Use the helpers in `kit` for input text, errors and
   calls to `llm`, `workspace` and other plugins.
3. Add it to `examples/cli-harness/manifest.json` under `plugins`: `name`,
   `description` (the lead agent's model reads this to decide when to ask),
   `agent` (path to the `.wasm`), and `grants`.
4. Grants are `llm`, `workspace`, `plugins`. Give the fewest that work. No
   grants is a valid choice.
5. `examples/cli-harness/build.sh`, then try it directly, with no model:
   `./target/release/agentwos-host --harness examples/cli-harness/manifest.json --project <dir> --message "/<name> <input>"`.
6. Add a line to `examples/cli-harness/smoke.sh` and run it.

## A standalone component (`modules/*` or inside an example)

1. Copy the closest existing crate. Keep the empty `[workspace]` table at the top
   of `Cargo.toml`: it keeps the crate out of the root (native) workspace.
2. Fix the relative `path` in `wit_bindgen::generate!` so it reaches
   `wit/world.wit` from the new location.
3. `cargo build --release --target wasm32-wasip2` in the crate.
4. If it belongs to an example, add it to that example's `build.sh` and
   manifest.
5. For the browser, the example's `build.sh` also transpiles it with `jco`; follow
   the pattern in `examples/tab-jam/build.sh`.

## Changing `wit/world.wit`

1. Edit the one file. Prefer adding an interface or a world to changing an
   existing signature: every guest of a changed world must be rebuilt and may
   need code changes.
2. Rebuild in this order: affected `modules/*`, each affected example's
   `build.sh`, then `cargo build --release`.
3. If the host implements the changed interface, update its `Host` trait
   implementation (`host/src/runtime.rs`, `host/src/harness.rs`).
4. If the browser kernel uses it, update `web/kernel/kernel.js` and rebuild the
   browser examples.
5. Run every smoke test in [verify.md](verify.md).

## Things that go wrong

| Symptom | Cause |
|---|---|
| `cargo` tries to build a guest for the host target | The crate lost its empty `[workspace]` table, or was added to the root workspace |
| Host fails to instantiate with a missing import | The guest was built against an older `world.wit`; rebuild it |
| Browser fails to instantiate after a rebuild | Stale files; serve with `tools/serve.py` and reload |
| The model ignores a tool or picks wrong arguments | Tool JSON key order, wording of the description, or too many tools for one agent. Test wordings against the model before changing code around it |
