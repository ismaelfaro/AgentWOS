# Build and verify

If `cargo` is not on `PATH`: `source "$HOME/.cargo/env"`.

## What needs what

| Check | Command | Needs |
|---|---|---|
| Host unit tests | `cargo test -p agentwos-host` | Nothing |
| Coding harness | `examples/cli-harness/build.sh && examples/cli-harness/smoke.sh` | Nothing; plugins are called directly |
| Mars colony | `examples/mars-colony/build.sh`, then `./target/release/agentwos-host --manifest examples/mars-colony/manifest.json --colony 1` | A model: LM Studio on `localhost:1234` or `OPENROUTER_API_KEY`. About 2 to 3 minutes per simulated day on a local model |
| Tab Jam kernel | `examples/tab-jam/build.sh`, then `(cd examples/tab-jam/web && node test/smoke.mjs)` | Node; the Needle model, fetched once |
| Tab Jam in a browser | `tools/serve.py`, open `http://localhost:8765/examples/tab-jam/web/dist/index.html` | A browser |

Other examples have their own `build.sh` and, where present, a smoke script.
Read the header comment of each script; it says what must be built first.

## Order

1. Build what you changed, then what depends on it (guest, example, host).
2. Run the checks that need nothing.
3. Run the model-backed checks if a model is available. If not, say which ones
   were skipped.
4. For anything visible, use it in a browser: the main path, one edge case, and
   the console for errors.

## Browser checks

- Use `tools/serve.py`, not `python3 -m http.server`: without no-cache headers a
  browser can pair new JavaScript with old `.wasm` and fail in confusing ways.
- Port 8765 may already be taken by a preview server. Reuse it or pass another
  port: `tools/serve.py 8766`.
- Audio cannot be judged by an agent. Offline rendering can show levels and
  clipping; whether it sounds right is for a person. Say so.

## Long runs

A multi-day colony run takes many minutes. Run it in the background, write its
output to a file, and report when it ends. Do not poll.

## Reporting

State what ran and its result, then what did not run and why. A check that was
skipped is reported as skipped.
