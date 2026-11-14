// Needle (Cactus Compute) as the kernel's `tool-llm` service. The engine is an
// Emscripten module exposing the C API in cactus.h; it runs fully in the browser.

const OUT_CAPACITY = 16384;
const MAX_NEW_TOKENS = 96;

/**
 * @param createCactus  factory exported by cactus.js
 * @param wasmBinary    bytes of cactus.wasm
 * @param modelBytes    bytes of needle3.cact
 * @returns an object matching the `agentwos:core/tool-llm` import
 */
export async function createNeedle({ createCactus, wasmBinary, modelBytes }) {
  const M = await createCactus({ wasmBinary });

  const modelPtr = M._malloc(modelBytes.length);
  M.HEAPU8.set(modelBytes, modelPtr);
  M.ccall("cactus_load", "number", ["number", "number"], [modelPtr, BigInt(modelBytes.length)]);
  if (M.ccall("cactus_models", "number", [], []) < 1) {
    throw new Error("Needle failed to load: " + M.ccall("cactus_last_error", "string", [], []));
  }
  const out = M._malloc(OUT_CAPACITY);

  return {
    call(system, toolsJson, input) {
      // The engine keeps conversation history; reset so each call stands alone.
      M.ccall("cactus_reset", null, [], []);
      M.ccall("cactus_init", "number", ["string", "string", "number"], [system, toolsJson, 0]);
      const written = M.ccall(
        "cactus_complete",
        "number",
        ["string", "number", "number", "number", "number", "number"],
        [input, 0, 0, MAX_NEW_TOKENS, out, OUT_CAPACITY],
      );
      if (written < 0) {
        throw { tag: "execution-failed", val: M.ccall("cactus_last_error", "string", [], []) };
      }
      return M.UTF8ToString(out);
    },
  };
}

/** Downloads a file with progress, keeping a copy in the Cache API for offline use. */
export async function fetchCached(url, cacheName, onProgress = () => {}) {
  const cache = await caches.open(cacheName);
  let response = await cache.match(url);
  if (!response) {
    response = await fetch(url);
    if (!response.ok) throw new Error(`download failed (${response.status}): ${url}`);
    const total = Number(response.headers.get("content-length")) || 0;
    const reader = response.body.getReader();
    const chunks = [];
    let loaded = 0;
    for (;;) {
      const { done, value } = await reader.read();
      if (done) break;
      chunks.push(value);
      loaded += value.length;
      onProgress(loaded, total);
    }
    const blob = new Blob(chunks);
    await cache.put(url, new Response(blob, { headers: { "content-length": String(blob.size) } }));
    return new Uint8Array(await blob.arrayBuffer());
  }
  return new Uint8Array(await response.arrayBuffer());
}
