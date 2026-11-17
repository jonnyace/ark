import { expect } from "@playwright/test";
import { readFileSync } from "node:fs";
import { test, installFromAddress, uri } from "./helpers";

// Criterion 10 (browser part): data from an older schema is migrated with a backup kept; bookmarks and notes export.
test("personal data from schema 1 is migrated, kept and exportable", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  // Play an older app: write a schema-1 record directly.
  await page.evaluate(() => new Promise<void>((res, rej) => {
    const r = indexedDB.open("lastbook-ark");
    r.onsuccess = () => {
      const tx = r.result.transaction("kv", "readwrite");
      tx.objectStore("kv").put({ schema: 1, data: { bookmarks: [{ id: "b1", pack: "org.lastbook.sample.docs", resource: "supplies-checklist", page: 2, title: "Old bookmark", created: "2026-01-01" }], history: [], settings: { ai_enabled: false } } }, "personal");
      tx.oncomplete = () => res();
      tx.onerror = () => rej(tx.error);
    };
  }));
  await page.goto("/#/saved");
  await page.reload();
  await expect(page.getByRole("link", { name: "Old bookmark" })).toHaveAttribute("href", /supplies-checklist%23page%3D2/);
  const backups = await page.evaluate(() => new Promise<string[]>((res) => {
    const r = indexedDB.open("lastbook-ark");
    r.onsuccess = () => { const q = r.result.transaction("kv").objectStore("kv").getAllKeys(); q.onsuccess = () => res(q.result as string[]); };
  }));
  expect(backups.some((k) => /^backup-\d+-v1$/.test(k))).toBe(true);

  await installFromAddress(page, "http://127.0.0.1:18787/packs/org.lastbook.sample.docs/pack.json");
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"]')).toBeVisible();
  await page.goto("/" + uri("lbn:org.lastbook.sample.docs/water-guide#sec=boil"));
  await page.getByRole("button", { name: /Bookmark this place/ }).click();
  await page.getByLabel("Add a note about this place").fill("Boil 3 minutes up here.");
  await page.getByRole("button", { name: "Save note" }).click();
  await page.goto("/#/saved");
  const dl = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export to a file…" }).click();
  const file = await (await dl).path();
  const data = JSON.parse(readFileSync(file, "utf8"));
  expect(data.format).toBe("lastbook-personal");
  expect(data.schema).toBe(2);
  expect(data.data.bookmarks.map((b: any) => b.target)).toContain("lbn:org.lastbook.sample.docs/water-guide#sec=boil");
  expect(data.data.notes[0].text).toBe("Boil 3 minutes up here.");
});
