import { test, expect } from "@playwright/test";
import { installFromAddress, server } from "./helpers";

const P = "http://127.0.0.1:18787/packs/";

// Criterion 8: tampered packs never become active.
test("tampered pack is rejected before activation", async ({ page }) => {
  await installFromAddress(page, P + "docs-v2-tampered/pack.json");
  await expect(page.locator('[data-testid="job"][data-state="failed"]')).toContainText("checksum");
  await expect(page.locator('[data-testid="pack"]')).toHaveCount(0);
  // A transfer corrupted on the way is also caught.
  await server(page, "/__fault", { path: "/packs/org.lastbook.sample.video/res/storing-water.webm", corrupt: true });
  await installFromAddress(page, P + "org.lastbook.sample.video/pack.json");
  await expect(page.locator('[data-testid="job"][data-state="failed"]').last()).toContainText("checksum");
  await expect(page.locator('[data-testid="pack"]')).toHaveCount(0);
});

// Criterion 7: an interrupted download resumes with a byte range; one that keeps failing is kept and resumes later.
test("interrupted download resumes and recovers after a reload", async ({ page }) => {
  const file = "/packs/org.lastbook.sample.video/res/storing-water.webm";
  await server(page, "/__fault", { path: file, cutAfter: 300_000 });
  await installFromAddress(page, P + "org.lastbook.sample.video/pack.json");
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.video"]')).toBeVisible({ timeout: 60_000 });
  const log: [string, string | null][] = await (await page.request.get("http://127.0.0.1:18787/__log")).json();
  const ranges = log.filter(([p]) => p === file).map(([, r]) => r);
  expect(ranges[0]).toBeNull();
  console.log("ranges", JSON.stringify(ranges));
  expect(ranges.some((r) => r && /^bytes=[1-9]\d*-$/.test(r))).toBe(true);

  // Now the publisher goes away mid-download: the job fails but keeps its partial file; after a reload it resumes.
  await page.goto("/#/packs");
  await page.locator('[data-pack="org.lastbook.sample.video"] button.danger').click({ trial: true });
  page.once("dialog", (d) => d.accept());
  await page.locator('[data-pack="org.lastbook.sample.video"] button.danger').click();
  await expect(page.locator('[data-testid="pack"]')).toHaveCount(0);
  await server(page, "/__fault", { path: file, cutAfter: 400_000 });
  await installFromAddress(page, P + "org.lastbook.sample.video/pack.json");
  await expect.poll(async () => (await (await page.request.get("http://127.0.0.1:18787/__log")).json()).filter(([p]: string[]) => p === file).length).toBeGreaterThan(ranges.length);
  await server(page, "/__down", { down: true });
  await expect(page.locator('[data-testid="job"][data-state="failed"]')).toBeVisible({ timeout: 30_000 });
  await page.reload();
  await expect(page.locator('[data-testid="job"]')).toContainText(/failed|paused/);
  await server(page, "/__down", { down: false });
  await page.getByRole("button", { name: /Retry|Resume/ }).click();
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.video"]')).toBeVisible({ timeout: 60_000 });
  const log2: [string, string | null][] = await (await page.request.get("http://127.0.0.1:18787/__log")).json();
  expect(log2.filter(([p]) => p === file).slice(ranges.length).some(([, r]) => r && /^bytes=\d{5,}-$/.test(r))).toBe(true);
});

// Criterion 9: a failed update leaves the previous version working; a good one can be rolled back.
test("failed update keeps the old version; rollback works", async ({ page }) => {
  await installFromAddress(page, P + "org.lastbook.sample.docs/pack.json");
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"]')).toContainText("v2026.11.17");
  await installFromAddress(page, P + "docs-v2-tampered/pack.json");
  await expect(page.locator('[data-testid="job"][data-state="failed"]')).toBeVisible();
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"]')).toContainText("v2026.11.17");
  await page.goto("/#/search?q=" + encodeURIComponent("gallon per person"));
  await expect(page.locator("main ol li").first()).not.toContainText("UPDATED");
  await installFromAddress(page, P + "docs-v2/pack.json");
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"]')).toContainText("v2026.11.18", { timeout: 30_000 });
  await page.goto("/#/search?q=" + encodeURIComponent("updated edition"));
  await expect(page.locator("main ol li").first()).toBeVisible();
  await page.goto("/#/packs");
  await page.getByRole("button", { name: "Go back to 2026.11.17" }).click();
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"]')).toContainText("v2026.11.17");
});

// Browser storage can be evicted: missing files are detected and repaired from the pack's source.
test("evicted files are detected and repaired", async ({ page }) => {
  await installFromAddress(page, P + "org.lastbook.sample.docs/pack.json");
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"]')).toBeVisible();
  await page.evaluate(async () => {
    const root = await navigator.storage.getDirectory();
    const builds = await root.getDirectoryHandle("builds");
    // @ts-ignore
    for await (const [name, h] of builds.entries()) await (await h.getDirectoryHandle("res")).removeEntry("supplies-checklist.pdf");
  });
  await page.goto("/#/");
  await page.reload();
  await expect(page.getByRole("alert")).toContainText("damaged");
  await page.goto("/#/packs");
  await page.getByRole("button", { name: "Repair" }).click();
  await expect(page.locator('[data-testid="pack"][data-pack="org.lastbook.sample.docs"] .badge.ok')).toBeVisible({ timeout: 30_000 });
});
