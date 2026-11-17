import { test, expect, chromium } from "@playwright/test";
import { spawn } from "node:child_process";
import { cpSync, mkdtempSync, readFileSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// A failed app update must leave the previous version working: publish a "new release" whose service worker lists a
// file the server cannot deliver; the new worker fails to install and the old one keeps serving the app, also offline.
test("a broken app update leaves the installed version working", async ({ browserName }) => {
  test.skip(browserName !== "chromium", "service-worker update behaviour checked once");
  test.setTimeout(120_000);
  const site = mkdtempSync(join(tmpdir(), "lbn-site-"));
  cpSync("../../apps/web/dist", site, { recursive: true });
  const port = 18792;
  const srv = spawn("node", ["server.mjs", String(port), site, "../../dist/test-packs"], { stdio: "ignore" });
  const profile = mkdtempSync(join(tmpdir(), "lbn-upd-"));
  const base = `http://127.0.0.1:${port}/`;
  const version = (page: any) => page.evaluate(() => new Promise<string>((res) => {
    const ch = new MessageChannel();
    ch.port1.onmessage = (m) => res(m.data);
    navigator.serviceWorker.controller!.postMessage("version", [ch.port2]);
  }));
  try {
    await new Promise((r) => setTimeout(r, 800));
    let ctx = await chromium.launchPersistentContext(profile);
    let page = ctx.pages()[0] ?? (await ctx.newPage());
    await page.goto(base);
    await page.evaluate(() => navigator.serviceWorker.ready);
    await page.reload();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your offline library");
    const v1 = await version(page);
    // "Release" a broken update: new service worker version that precaches a file which does not exist.
    const sw = readFileSync(join(site, "sw.js"), "utf8").replace(/const VERSION = "[^"]+"/, 'const VERSION = "broken-update"').replace("const PRECACHE = [", 'const PRECACHE = ["assets/missing-file.js",');
    writeFileSync(join(site, "sw.js"), sw);
    await page.evaluate(async () => (await navigator.serviceWorker.getRegistration())!.update().catch(() => {}));
    await page.waitForTimeout(2000);
    await page.reload();
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your offline library");
    expect(await version(page)).toBe(v1);
    await ctx.close();
    srv.kill();
    ctx = await chromium.launchPersistentContext(profile, { offline: true });
    page = ctx.pages()[0] ?? (await ctx.newPage());
    await page.goto(base);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your offline library");
    expect(await version(page)).toBe(v1);
    await ctx.close();
  } finally {
    srv.kill();
    rmSync(site, { recursive: true, force: true });
    rmSync(profile, { recursive: true, force: true });
  }
});
