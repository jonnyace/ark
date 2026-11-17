import { defineConfig } from "@playwright/test";

// Browser suite for the PWA. The server serves the built app and dist/test-packs (tests/e2e/make-test-packs.sh).
export default defineConfig({
  testDir: "browser",
  timeout: 120_000,
  workers: 1,
  reporter: [["list"], ["json", { outputFile: "../../.dev-cache/e2e/browser-results.json" }]],
  // LBN_HEADED=1 runs real windows (e.g. under xvfb-run), which gives Firefox WebGL for the map.
  use: { baseURL: "http://127.0.0.1:18787/", viewport: { width: 1100, height: 900 }, screenshot: "only-on-failure", headless: !process.env.LBN_HEADED },
  outputDir: "../../.dev-cache/e2e/browser-output",
  projects: [
    { name: "chromium", use: { browserName: "chromium" } },
    // Compatibility runs: everything except the long real-model test, which needs Chromium features (JSPI, memory64).
    { name: "firefox", use: { browserName: "firefox" }, testIgnore: /offline\.spec/ },
    { name: "webkit", use: { browserName: "webkit" }, testIgnore: /offline\.spec/ },
  ],
  webServer: {
    command: "node server.mjs 18787 ../../apps/web/dist ../../dist/test-packs",
    url: "http://127.0.0.1:18787/",
    reuseExistingServer: false,
  },
});
