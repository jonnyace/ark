// Ark service worker (generated from src/sw-template.js at build time).
//
// - Precaches every file of this app build, so the app starts with all networks off.
// - A new build installs into a new cache; if that install fails (network lost half-way), the old worker and its
//   cache stay in charge, so a failed app update never leaves a broken app.
// - Library packs are NOT stored here: they live in the origin private file system (OPFS) managed by the app.
//   Requests for packs and catalogs always go to the network.
// - Responses served from the cache carry COOP/COEP headers so the page can be cross-origin isolated, which the
//   in-browser AI runtime needs for multi-threading. Nothing here contacts any other origin.
const VERSION = "6600afcfde110b78";
const PRECACHE = ["assets/index-CCMSOS9S.js","assets/index-D5becoUc.css","assets/index-DyKXT6S_.js","assets/lbn_bg-DNNNQtNX.wasm","assets/maplibre-gl-worker-ScjyBlZc.js","assets/pdf.worker.min-CIqJB0yW.mjs","assets/transport-tauri-CMep6DNH.js","assets/transport-worker-yXKWmLx-.js","assets/wllama-CO25czjY.wasm","icons/icon-128.png","icons/icon-192.png","icons/icon-256.png","icons/icon-32.png","icons/icon-512.png","icons/icon.svg","index.html","manifest.webmanifest","pdfjs/cmaps/78-EUC-H.bcmap","pdfjs/cmaps/78-EUC-V.bcmap","pdfjs/cmaps/78-H.bcmap","pdfjs/cmaps/78-RKSJ-H.bcmap","pdfjs/cmaps/78-RKSJ-V.bcmap","pdfjs/cmaps/78-V.bcmap","pdfjs/cmaps/78ms-RKSJ-H.bcmap","pdfjs/cmaps/78ms-RKSJ-V.bcmap","pdfjs/cmaps/83pv-RKSJ-H.bcmap","pdfjs/cmaps/90ms-RKSJ-H.bcmap","pdfjs/cmaps/90ms-RKSJ-V.bcmap","pdfjs/cmaps/90msp-RKSJ-H.bcmap","pdfjs/cmaps/90msp-RKSJ-V.bcmap","pdfjs/cmaps/90pv-RKSJ-H.bcmap","pdfjs/cmaps/90pv-RKSJ-V.bcmap","pdfjs/cmaps/Add-H.bcmap","pdfjs/cmaps/Add-RKSJ-H.bcmap","pdfjs/cmaps/Add-RKSJ-V.bcmap","pdfjs/cmaps/Add-V.bcmap","pdfjs/cmaps/Adobe-CNS1-0.bcmap","pdfjs/cmaps/Adobe-CNS1-1.bcmap","pdfjs/cmaps/Adobe-CNS1-2.bcmap","pdfjs/cmaps/Adobe-CNS1-3.bcmap","pdfjs/cmaps/Adobe-CNS1-4.bcmap","pdfjs/cmaps/Adobe-CNS1-5.bcmap","pdfjs/cmaps/Adobe-CNS1-6.bcmap","pdfjs/cmaps/Adobe-CNS1-UCS2.bcmap","pdfjs/cmaps/Adobe-GB1-0.bcmap","pdfjs/cmaps/Adobe-GB1-1.bcmap","pdfjs/cmaps/Adobe-GB1-2.bcmap","pdfjs/cmaps/Adobe-GB1-3.bcmap","pdfjs/cmaps/Adobe-GB1-4.bcmap","pdfjs/cmaps/Adobe-GB1-5.bcmap","pdfjs/cmaps/Adobe-GB1-UCS2.bcmap","pdfjs/cmaps/Adobe-Japan1-0.bcmap","pdfjs/cmaps/Adobe-Japan1-1.bcmap","pdfjs/cmaps/Adobe-Japan1-2.bcmap","pdfjs/cmaps/Adobe-Japan1-3.bcmap","pdfjs/cmaps/Adobe-Japan1-4.bcmap","pdfjs/cmaps/Adobe-Japan1-5.bcmap","pdfjs/cmaps/Adobe-Japan1-6.bcmap","pdfjs/cmaps/Adobe-Japan1-UCS2.bcmap","pdfjs/cmaps/Adobe-Korea1-0.bcmap","pdfjs/cmaps/Adobe-Korea1-1.bcmap","pdfjs/cmaps/Adobe-Korea1-2.bcmap","pdfjs/cmaps/Adobe-Korea1-UCS2.bcmap","pdfjs/cmaps/B5-H.bcmap","pdfjs/cmaps/B5-V.bcmap","pdfjs/cmaps/B5pc-H.bcmap","pdfjs/cmaps/B5pc-V.bcmap","pdfjs/cmaps/CNS-EUC-H.bcmap","pdfjs/cmaps/CNS-EUC-V.bcmap","pdfjs/cmaps/CNS1-H.bcmap","pdfjs/cmaps/CNS1-V.bcmap","pdfjs/cmaps/CNS2-H.bcmap","pdfjs/cmaps/CNS2-V.bcmap","pdfjs/cmaps/ETHK-B5-H.bcmap","pdfjs/cmaps/ETHK-B5-V.bcmap","pdfjs/cmaps/ETen-B5-H.bcmap","pdfjs/cmaps/ETen-B5-V.bcmap","pdfjs/cmaps/ETenms-B5-H.bcmap","pdfjs/cmaps/ETenms-B5-V.bcmap","pdfjs/cmaps/EUC-H.bcmap","pdfjs/cmaps/EUC-V.bcmap","pdfjs/cmaps/Ext-H.bcmap","pdfjs/cmaps/Ext-RKSJ-H.bcmap","pdfjs/cmaps/Ext-RKSJ-V.bcmap","pdfjs/cmaps/Ext-V.bcmap","pdfjs/cmaps/GB-EUC-H.bcmap","pdfjs/cmaps/GB-EUC-V.bcmap","pdfjs/cmaps/GB-H.bcmap","pdfjs/cmaps/GB-V.bcmap","pdfjs/cmaps/GBK-EUC-H.bcmap","pdfjs/cmaps/GBK-EUC-V.bcmap","pdfjs/cmaps/GBK2K-H.bcmap","pdfjs/cmaps/GBK2K-V.bcmap","pdfjs/cmaps/GBKp-EUC-H.bcmap","pdfjs/cmaps/GBKp-EUC-V.bcmap","pdfjs/cmaps/GBT-EUC-H.bcmap","pdfjs/cmaps/GBT-EUC-V.bcmap","pdfjs/cmaps/GBT-H.bcmap","pdfjs/cmaps/GBT-V.bcmap","pdfjs/cmaps/GBTpc-EUC-H.bcmap","pdfjs/cmaps/GBTpc-EUC-V.bcmap","pdfjs/cmaps/GBpc-EUC-H.bcmap","pdfjs/cmaps/GBpc-EUC-V.bcmap","pdfjs/cmaps/H.bcmap","pdfjs/cmaps/HKdla-B5-H.bcmap","pdfjs/cmaps/HKdla-B5-V.bcmap","pdfjs/cmaps/HKdlb-B5-H.bcmap","pdfjs/cmaps/HKdlb-B5-V.bcmap","pdfjs/cmaps/HKgccs-B5-H.bcmap","pdfjs/cmaps/HKgccs-B5-V.bcmap","pdfjs/cmaps/HKm314-B5-H.bcmap","pdfjs/cmaps/HKm314-B5-V.bcmap","pdfjs/cmaps/HKm471-B5-H.bcmap","pdfjs/cmaps/HKm471-B5-V.bcmap","pdfjs/cmaps/HKscs-B5-H.bcmap","pdfjs/cmaps/HKscs-B5-V.bcmap","pdfjs/cmaps/Hankaku.bcmap","pdfjs/cmaps/Hiragana.bcmap","pdfjs/cmaps/KSC-EUC-H.bcmap","pdfjs/cmaps/KSC-EUC-V.bcmap","pdfjs/cmaps/KSC-H.bcmap","pdfjs/cmaps/KSC-Johab-H.bcmap","pdfjs/cmaps/KSC-Johab-V.bcmap","pdfjs/cmaps/KSC-V.bcmap","pdfjs/cmaps/KSCms-UHC-H.bcmap","pdfjs/cmaps/KSCms-UHC-HW-H.bcmap","pdfjs/cmaps/KSCms-UHC-HW-V.bcmap","pdfjs/cmaps/KSCms-UHC-V.bcmap","pdfjs/cmaps/KSCpc-EUC-H.bcmap","pdfjs/cmaps/KSCpc-EUC-V.bcmap","pdfjs/cmaps/Katakana.bcmap","pdfjs/cmaps/LICENSE","pdfjs/cmaps/NWP-H.bcmap","pdfjs/cmaps/NWP-V.bcmap","pdfjs/cmaps/RKSJ-H.bcmap","pdfjs/cmaps/RKSJ-V.bcmap","pdfjs/cmaps/Roman.bcmap","pdfjs/cmaps/UniCNS-UCS2-H.bcmap","pdfjs/cmaps/UniCNS-UCS2-V.bcmap","pdfjs/cmaps/UniCNS-UTF16-H.bcmap","pdfjs/cmaps/UniCNS-UTF16-V.bcmap","pdfjs/cmaps/UniCNS-UTF32-H.bcmap","pdfjs/cmaps/UniCNS-UTF32-V.bcmap","pdfjs/cmaps/UniCNS-UTF8-H.bcmap","pdfjs/cmaps/UniCNS-UTF8-V.bcmap","pdfjs/cmaps/UniGB-UCS2-H.bcmap","pdfjs/cmaps/UniGB-UCS2-V.bcmap","pdfjs/cmaps/UniGB-UTF16-H.bcmap","pdfjs/cmaps/UniGB-UTF16-V.bcmap","pdfjs/cmaps/UniGB-UTF32-H.bcmap","pdfjs/cmaps/UniGB-UTF32-V.bcmap","pdfjs/cmaps/UniGB-UTF8-H.bcmap","pdfjs/cmaps/UniGB-UTF8-V.bcmap","pdfjs/cmaps/UniJIS-UCS2-H.bcmap","pdfjs/cmaps/UniJIS-UCS2-HW-H.bcmap","pdfjs/cmaps/UniJIS-UCS2-HW-V.bcmap","pdfjs/cmaps/UniJIS-UCS2-V.bcmap","pdfjs/cmaps/UniJIS-UTF16-H.bcmap","pdfjs/cmaps/UniJIS-UTF16-V.bcmap","pdfjs/cmaps/UniJIS-UTF32-H.bcmap","pdfjs/cmaps/UniJIS-UTF32-V.bcmap","pdfjs/cmaps/UniJIS-UTF8-H.bcmap","pdfjs/cmaps/UniJIS-UTF8-V.bcmap","pdfjs/cmaps/UniJIS2004-UTF16-H.bcmap","pdfjs/cmaps/UniJIS2004-UTF16-V.bcmap","pdfjs/cmaps/UniJIS2004-UTF32-H.bcmap","pdfjs/cmaps/UniJIS2004-UTF32-V.bcmap","pdfjs/cmaps/UniJIS2004-UTF8-H.bcmap","pdfjs/cmaps/UniJIS2004-UTF8-V.bcmap","pdfjs/cmaps/UniJISPro-UCS2-HW-V.bcmap","pdfjs/cmaps/UniJISPro-UCS2-V.bcmap","pdfjs/cmaps/UniJISPro-UTF8-V.bcmap","pdfjs/cmaps/UniJISX0213-UTF32-H.bcmap","pdfjs/cmaps/UniJISX0213-UTF32-V.bcmap","pdfjs/cmaps/UniJISX02132004-UTF32-H.bcmap","pdfjs/cmaps/UniJISX02132004-UTF32-V.bcmap","pdfjs/cmaps/UniKS-UCS2-H.bcmap","pdfjs/cmaps/UniKS-UCS2-V.bcmap","pdfjs/cmaps/UniKS-UTF16-H.bcmap","pdfjs/cmaps/UniKS-UTF16-V.bcmap","pdfjs/cmaps/UniKS-UTF32-H.bcmap","pdfjs/cmaps/UniKS-UTF32-V.bcmap","pdfjs/cmaps/UniKS-UTF8-H.bcmap","pdfjs/cmaps/UniKS-UTF8-V.bcmap","pdfjs/cmaps/V.bcmap","pdfjs/cmaps/WP-Symbol.bcmap","pdfjs/standard_fonts/FoxitDingbats.pfb","pdfjs/standard_fonts/FoxitFixed.pfb","pdfjs/standard_fonts/FoxitFixedBold.pfb","pdfjs/standard_fonts/FoxitFixedBoldItalic.pfb","pdfjs/standard_fonts/FoxitFixedItalic.pfb","pdfjs/standard_fonts/FoxitSerif.pfb","pdfjs/standard_fonts/FoxitSerifBold.pfb","pdfjs/standard_fonts/FoxitSerifBoldItalic.pfb","pdfjs/standard_fonts/FoxitSerifItalic.pfb","pdfjs/standard_fonts/FoxitSymbol.pfb","pdfjs/standard_fonts/LICENSE_FOXIT","pdfjs/standard_fonts/LICENSE_LIBERATION","pdfjs/standard_fonts/LiberationSans-Bold.ttf","pdfjs/standard_fonts/LiberationSans-BoldItalic.ttf","pdfjs/standard_fonts/LiberationSans-Italic.ttf","pdfjs/standard_fonts/LiberationSans-Regular.ttf"];
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
