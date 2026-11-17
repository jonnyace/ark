import { test, expect, chromium, firefox, webkit } from "@playwright/test";
import { spawn } from "node:child_process";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { uri } from "./helpers";

// Criterion 3/4: after one online visit and installing packs, the PWA starts with the network off and the server gone,
// in a NEW browser process using the same profile, and search/documents/video/map all work.
test("cold start offline after installation, server stopped", async ({ browserName }) => {
  const engine = { chromium, firefox, webkit }[browserName];
  test.setTimeout(240_000);
  const port = { chromium: 18790, firefox: 18793, webkit: 18794 }[browserName];
  const srv = spawn("node", ["server.mjs", String(port), "../../apps/web/dist", "../../dist/test-packs"], { stdio: "ignore" });
  const base = `http://127.0.0.1:${port}/`;
  const profile = mkdtempSync(join(tmpdir(), "lbn-pwa-"));
  try {
    await new Promise((r) => setTimeout(r, 800));
    let ctx = await engine.launchPersistentContext(profile, { viewport: { width: 1100, height: 900 } });
    let page = ctx.pages()[0] ?? (await ctx.newPage());
    await page.goto(base);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your offline library");
    await page.evaluate(() => navigator.serviceWorker.ready);
    await page.goto(base + "#/packs");
    await page.locator("#cat-url").fill(base + "packs/catalog.json");
    await page.getByRole("button", { name: "Show packs" }).click();
    for (const id of ["docs", "video", "map"]) {
      await page.locator(`[data-testid="catalog-pack"][data-pack="org.lastbook.sample.${id}"] button`).click();
      await page.getByRole("button", { name: "Install", exact: true }).click();
      await expect(page.locator(`[data-testid="pack"][data-pack="org.lastbook.sample.${id}"]`)).toBeVisible({ timeout: 60_000 });
    }
    // Make sure the new service worker version has finished precaching before going offline.
    await expect.poll(() => page.evaluate(async () => !!(await navigator.serviceWorker.getRegistration())?.active)).toBe(true);
    await ctx.close();
    srv.kill();
    await new Promise((r) => setTimeout(r, 500));

    ctx = await engine.launchPersistentContext(profile, { viewport: { width: 1100, height: 900 }, offline: true });
    page = ctx.pages()[0] ?? (await ctx.newPage());
    await page.goto(base);
    await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your offline library");
    await expect(page.locator(".net")).toContainText("Offline");
    expect(await page.evaluate(() => fetch(`/packs/never-requested-${Date.now()}.json`, { cache: "no-store" }).then(() => "reached", () => "unreachable"))).toBe("unreachable");
    await page.goto(base + "#/search?q=boil");
    await expect(page.locator("main ol li").first()).toBeVisible();
    await page.goto(base + uri("lbn:org.lastbook.sample.docs/supplies-checklist#page=3"));
    await expect(page.getByTestId("pdf-text")).toContainText("six months", { useInnerText: false });
    await page.goto(base + uri("lbn:org.lastbook.sample.video/storing-water#t=20"));
    const v = page.getByTestId("video");
    await expect.poll(() => v.evaluate((e: HTMLVideoElement) => [e.readyState >= 1, Math.round(e.currentTime)])).toEqual([true, 20]);
    await page.goto(base + uri("lbn:org.lastbook.sample.map/ithaca"));
    if (await page.evaluate(() => !!document.createElement("canvas").getContext("webgl2"))) {
      await expect(page.locator('[data-testid="map"][data-loaded="true"]')).toBeVisible({ timeout: 30_000 });
    } else {
      // Headless Firefox has no WebGL; the headed run (ci.sh, LBN_HEADED=1) covers drawing.
      await expect(page.getByRole("alert")).toContainText("cannot draw maps: WebGL is not available");
    }
    await ctx.close();
  } finally {
    srv.kill();
    rmSync(profile, { recursive: true, force: true });
  }
});
