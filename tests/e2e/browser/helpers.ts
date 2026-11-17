import { expect, test as base, chromium, firefox, webkit, type Page } from "@playwright/test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// WebKit (like Safari) gives no file storage to private/ephemeral windows, so WebKit tests always run in a normal
// (persistent) profile. Tests that store large files (the 1.1 GB model) set `persistent: true` for every browser:
// Playwright's default contexts are incognito-like, and Chromium keeps incognito storage in memory, which fails on
// smaller machines (seen on GitHub's 16 GB runner) although a real browser profile stores on disk.
export const test = base.extend<{ persistent: boolean }>({
  persistent: [false, { option: true }],
  context: async ({ browserName, context, baseURL, viewport, persistent }, use) => {
    if (browserName !== "webkit" && !persistent) return use(context);
    const dir = mkdtempSync(join(tmpdir(), "lbn-p-"));
    const engine = { chromium, firefox, webkit }[browserName];
    const ctx = await engine.launchPersistentContext(dir, { baseURL, viewport: viewport ?? undefined, acceptDownloads: true });
    await use(ctx);
    await ctx.close();
    rmSync(dir, { recursive: true, force: true });
  },
});

export const CATALOG = "http://127.0.0.1:18787/packs/catalog.json";

export async function openCatalog(page: Page, base = "/") {
  await page.goto(base + "#/packs");
  await page.locator("#cat-url").fill(CATALOG.replace("http://127.0.0.1:18787/", new URL(base, page.url() || "http://127.0.0.1:18787/").href.replace(/#.*/, "")));
  await page.getByRole("button", { name: "Show packs" }).click();
  await expect(page.locator('[data-testid="catalog-pack"]').first()).toBeVisible();
}

export async function installFromCatalog(page: Page, id: string) {
  await page.locator(`[data-testid="catalog-pack"][data-pack="${id}"] button`).click();
  await expect(page.getByTestId("confirm")).toBeVisible();
  await page.getByRole("button", { name: "Install", exact: true }).click();
  await expect(page.locator(`[data-testid="pack"][data-pack="${id}"]`)).toBeVisible({ timeout: 300_000 });
}

export async function installFromAddress(page: Page, url: string, opts: { unsigned?: boolean } = {}) {
  await page.goto("/#/packs");
  await page.locator("#addr").fill(url);
  await page.getByRole("button", { name: "Check", exact: true }).click();
  await expect(page.getByTestId("confirm")).toBeVisible();
  if (opts.unsigned) await page.getByLabel("I trust where this pack came from").check();
  await page.getByRole("button", { name: "Install", exact: true }).click();
}

export async function server(page: Page, path: string, body: unknown) {
  await page.request.post(`http://127.0.0.1:18787${path}`, { data: body });
}

export const uri = (u: string) => "#/read?c=" + encodeURIComponent(u);
