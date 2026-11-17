// Ark service worker (generated from src/sw-template.js at build time).
//
// - Precaches every file of this app build, so the app starts with all networks off.
// - A new build installs into a new cache; if that install fails (network lost half-way), the old worker and its
//   cache stay in charge, so a failed app update never leaves a broken app.
// - Library packs are NOT stored here: they live in the origin private file system (OPFS) managed by the app.
//   Requests for packs and catalogs always go to the network.
// - Responses served from the cache carry COOP/COEP headers so the page can be cross-origin isolated, which the
//   in-browser AI runtime needs for multi-threading. Nothing here contacts any other origin.
const VERSION = "__VERSION__";
const PRECACHE = __PRECACHE__;
const CACHE = `lbn-app-${VERSION}`;

self.addEventListener("install", (event) => {
  event.waitUntil(
    (async () => {
      const cache = await caches.open(CACHE);
      // All-or-nothing: addAll rejects if any file fails, and the old version stays active.
      await cache.addAll(["./", ...PRECACHE].map((u) => new Request(u, { cache: "reload" })));
    })(),
  );
});

self.addEventListener("activate", (event) => {
  event.waitUntil(
    (async () => {
      for (const k of await caches.keys()) {
        if (k.startsWith("lbn-app-") && k !== CACHE) await caches.delete(k);
      }
      await self.clients.claim();
    })(),
  );
});

self.addEventListener("message", (event) => {
  if (event.data === "skip-waiting") self.skipWaiting();
  if (event.data === "version" && event.ports[0]) event.ports[0].postMessage(VERSION);
});

function isolate(resp) {
  const h = new Headers(resp.headers);
  h.set("Cross-Origin-Opener-Policy", "same-origin");
  h.set("Cross-Origin-Embedder-Policy", "require-corp");
  h.set("Cross-Origin-Resource-Policy", "same-origin");
  return new Response(resp.body, { status: resp.status, statusText: resp.statusText, headers: h });
}

self.addEventListener("fetch", (event) => {
  const req = event.request;
  if (req.method !== "GET") return;
  const url = new URL(req.url);
  if (url.origin !== self.location.origin) return;
  const scope = new URL(self.registration.scope);
  const rel = url.pathname.startsWith(scope.pathname) ? url.pathname.slice(scope.pathname.length) : null;
  if (rel === null) return;
  if (req.mode === "navigate") {
    event.respondWith(
      (async () => {
        const cache = await caches.open(CACHE);
        const hit = (await cache.match("./")) || (await cache.match("index.html"));
        if (hit) return isolate(hit);
        return isolate(await fetch(req));
      })(),
    );
    return;
  }
  if (!PRECACHE.includes(decodeURIComponent(rel))) return; // packs, catalogs, anything else: network
  event.respondWith(
    (async () => {
      const cache = await caches.open(CACHE);
      const hit = await cache.match(req, { ignoreSearch: true });
      return isolate(hit || (await fetch(req)));
    })(),
  );
});
