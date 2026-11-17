// Static test server for the browser app and sample packs (no dependencies).
//   node tests/e2e/server.mjs [port] [appDir] [packsDir]
// - Serves the built app at / with COOP/COEP (as a static host should) and packs at /packs/ with byte ranges.
// - Fault injection for tests: POST /__fault {"path": "/packs/x/file", "cutAfter": 100000} makes the next GET of that
//   path stop after that many bytes (connection destroyed). POST /__fault {"path": ..., "corrupt": true} flips a byte.
// - POST /__down {"down": true} makes every /packs request fail (simulates losing the connection to the publisher).
import http from "node:http";
import { createReadStream, statSync, existsSync } from "node:fs";
import { join, normalize, extname } from "node:path";

const port = +(process.argv[2] ?? 18787);
const appDir = process.argv[3] ?? new URL("../../apps/web/dist", import.meta.url).pathname;
const packsDir = process.argv[4] ?? new URL("../../dist/packs", import.meta.url).pathname;
const types = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".css": "text/css", ".json": "application/json",
  ".wasm": "application/wasm", ".webmanifest": "application/manifest+json", ".svg": "image/svg+xml", ".png": "image/png", ".webp": "image/webp",
  ".webm": "video/webm", ".pdf": "application/pdf", ".vtt": "text/vtt", ".bcmap": "application/octet-stream", ".pfb": "application/octet-stream" };
const faults = new Map();
let down = false;
const counts = new Map();

function send(req, res, file) {
  let st;
  try { st = statSync(file); } catch { res.writeHead(404).end("not found"); return; }
  if (st.isDirectory()) return send(req, res, join(file, "index.html"));
  const etag = `"${st.size}-${st.mtimeMs}"`;
  const headers = { "Content-Type": types[extname(file)] ?? "application/octet-stream", "Accept-Ranges": "bytes", ETag: etag,
    "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp", "Cross-Origin-Resource-Policy": "same-origin",
    "Cache-Control": "no-cache" };
  let start = 0, end = st.size - 1, status = 200;
  const m = /^bytes=(\d+)-(\d*)$/.exec(req.headers.range ?? "");
  if (m) {
    start = +m[1];
    end = m[2] ? Math.min(+m[2], st.size - 1) : st.size - 1;
    if (start >= st.size) { res.writeHead(416, { "Content-Range": `bytes */${st.size}` }).end(); return; }
    status = 206;
    headers["Content-Range"] = `bytes ${start}-${end}/${st.size}`;
  }
  headers["Content-Length"] = end - start + 1;
  res.writeHead(status, headers);
  if (req.method === "HEAD") return res.end();
  const f = faults.get(req.url);
  const s = createReadStream(file, { start, end });
  if (f) {
    faults.delete(req.url);
    let sent = 0;
    s.on("data", (chunk) => {
      if (f.corrupt) { chunk = Buffer.from(chunk); chunk[0] ^= 0xff; f.corrupt = false; }
      if (f.cutAfter != null && sent + chunk.length > f.cutAfter) {
        res.write(chunk.subarray(0, Math.max(0, f.cutAfter - sent)));
        s.destroy(); res.destroy(); return;
      }
      sent += chunk.length;
      res.write(chunk);
    });
    s.on("end", () => res.end());
  } else s.pipe(res);
}

http.createServer((req, res) => {
  const url = new URL(req.url, "http://x");
  if (req.method === "POST" && (url.pathname === "/__fault" || url.pathname === "/__down")) {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      const j = JSON.parse(body || "{}");
      if (url.pathname === "/__down") down = !!j.down; else faults.set(j.path, j);
      res.writeHead(200).end("ok");
    });
    return;
  }
  if (url.pathname === "/__counts") { res.writeHead(200, { "Content-Type": "application/json" }).end(JSON.stringify(Object.fromEntries(counts))); return; }
  counts.set(url.pathname, (counts.get(url.pathname) ?? 0) + 1);
  const p = decodeURIComponent(url.pathname);
  if (p.startsWith("/packs/")) {
    if (down) { res.destroy(); return; }
    const f = normalize(join(packsDir, p.slice(7)));
    if (!f.startsWith(normalize(packsDir))) { res.writeHead(403).end(); return; }
    return send(req, res, f);
  }
  const f = normalize(join(appDir, p === "/" ? "index.html" : p));
  if (!f.startsWith(normalize(appDir)) || !existsSync(f)) { res.writeHead(404).end("not found"); return; }
  send(req, res, f);
}).listen(port, "127.0.0.1", () => console.log(`serving ${appDir} and ${packsDir} on http://127.0.0.1:${port}`));
