import { defineConfig, type Plugin } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { createHash } from "node:crypto";
import { cpSync, readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { join, relative } from "node:path";

// Copies pdf.js font data and character maps next to the app (never fetched from a CDN), then writes the service
// worker with the full list of built files, so the browser app can start with every network disabled.
function offlineAssets(): Plugin {
  let outDir = "dist";
  return {
    name: "lbn-offline-assets",
    configResolved(c) {
      outDir = c.build.outDir;
    },
    closeBundle() {
      const pdfjs = join(import.meta.dirname, "node_modules/pdfjs-dist");
      cpSync(join(pdfjs, "standard_fonts"), join(outDir, "pdfjs/standard_fonts"), { recursive: true });
      cpSync(join(pdfjs, "cmaps"), join(outDir, "pdfjs/cmaps"), { recursive: true });
      const files: string[] = [];
      const walk = (d: string) => {
        for (const n of readdirSync(d)) {
          const p = join(d, n);
          if (statSync(p).isDirectory()) walk(p);
          else files.push(relative(outDir, p).split("\\").join("/"));
        }
      };
      walk(outDir);
      const precache = files.filter((f) => f !== "sw.js" && !f.endsWith(".map")).sort();
      const h = createHash("sha256");
      for (const f of precache) h.update(f).update(readFileSync(join(outDir, f)));
      const version = h.digest("hex").slice(0, 16);
      const tpl = readFileSync(join(import.meta.dirname, "src/sw-template.js"), "utf8");
      writeFileSync(join(outDir, "sw.js"), tpl.replace("__VERSION__", version).replace("__PRECACHE__", JSON.stringify(precache)));
      writeFileSync(join(outDir, "build-info.json"), JSON.stringify({ version, files: precache.length }) + "\n");
    },
  };
}

export default defineConfig({
  base: "./",
  plugins: [svelte(), offlineAssets()],
  build: { target: "es2022", sourcemap: false, chunkSizeWarningLimit: 4000, assetsInlineLimit: 0 },
  worker: { format: "es" },
  server: {
    headers: { "Cross-Origin-Opener-Policy": "same-origin", "Cross-Origin-Embedder-Policy": "require-corp" },
  },
});
