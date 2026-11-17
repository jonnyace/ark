// Render the app icon SVG to the PNG sizes the web manifest and desktop bundles need (uses Playwright's Chromium).
// Usage: node tests/e2e/render-icons.mjs apps/web/public/icons/icon.svg apps/web/public/icons
import { chromium } from "@playwright/test";
import { readFileSync } from "node:fs";
const [, , svgPath, outDir] = process.argv;
const svg = readFileSync(svgPath, "utf8");
const browser = await chromium.launch();
for (const size of [32, 128, 192, 256, 512]) {
  const page = await browser.newPage({ viewport: { width: size, height: size } });
  await page.setContent(`<html><body style="margin:0;background:transparent">${svg.replace("<svg ", `<svg width="${size}" height="${size}" `)}</body></html>`);
  await page.screenshot({ path: `${outDir}/icon-${size}.png`, omitBackground: true });
  await page.close();
}
await browser.close();
console.log("icons written to", outDir);
