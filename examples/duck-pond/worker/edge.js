// Edge Worker for Tab Jam. Static files come from the assets binding; the one
// route that needs code is the Needle model. The browser cannot always reach
// huggingface.co from this site, so the Worker fetches the file on the server,
// streams it to the page, and keeps a copy in the Cloudflare cache.

const MODEL_PATH = "/model/needle3.cact";
const MODEL_SOURCE = "https://huggingface.co/Cactus-Compute/needle3/resolve/main/needle3.cact";
const CACHE_SECONDS = 60 * 60 * 24 * 7;

async function model(request, ctx) {
  const cache = caches.default;
  const key = new Request(new URL(MODEL_PATH, request.url).href);
  const hit = await cache.match(key);
  if (hit) return hit;

  const upstream = await fetch(MODEL_SOURCE);
  if (!upstream.ok || !upstream.body) {
    return new Response(`model unavailable (upstream ${upstream.status})`, { status: 502 });
  }
  const headers = {
    "content-type": "application/octet-stream",
    "cache-control": `public, max-age=${CACHE_SECONDS}`,
  };
  const length = upstream.headers.get("content-length");
  if (length) headers["content-length"] = length;

  // One copy goes to the browser, the other to the cache, so the first visitor
  // does not wait for the whole file before the page can start reading it.
  const [toBrowser, toCache] = upstream.body.tee();
  ctx.waitUntil(cache.put(key, new Response(toCache, { headers })));
  return new Response(toBrowser, { headers });
}

export default {
  async fetch(request, env, ctx) {
    const { pathname } = new URL(request.url);
    if (pathname === MODEL_PATH) {
      if (request.method !== "GET" && request.method !== "HEAD") {
        return new Response("method not allowed", { status: 405, headers: { allow: "GET, HEAD" } });
      }
      return model(request, ctx);
    }
    return env.ASSETS.fetch(request);
  },
};
