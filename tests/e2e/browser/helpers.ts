import { expect, test as base, webkit, type Page } from "@playwright/test";
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

// WebKit (like Safari) gives no file storage to private/ephemeral windows, so WebKit tests run in a normal
// (persistent) profile. Other browsers use Playwright's default contexts.
export const test = base.extend({
  context: async ({ browserName, context, baseURL, viewport }, use) => {
    if (browserName !== "webkit") return use(context);
    const dir = mkdtempSync(join(tmpdir(), "lbn-wk-"));
    const ctx = await webkit.launchPersistentContext(dir, { baseURL, viewport: viewport ?? undefined, acceptDownloads: true });
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
