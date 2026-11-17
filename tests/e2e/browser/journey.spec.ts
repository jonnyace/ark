import { test, expect } from "@playwright/test";
import { openCatalog, installFromCatalog, uri } from "./helpers";

// Criterion 2/4 (browser part): install from a signed catalog, then search, read, watch and look at the map.
test("install sample packs and use every viewer", async ({ page }) => {
  await page.goto("/");
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Your offline library");
  await openCatalog(page);
  await expect(page.getByText("signed by Lastbook Next sample publisher (development)")).toBeVisible();
  for (const id of ["docs", "video", "map", "encyclopedia"]) await installFromCatalog(page, `org.lastbook.sample.${id}`);

  await page.goto("/#/search?q=" + encodeURIComponent("bleach drops gallon"));
  await expect(page.locator("main ol li a").first()).toHaveText("Making water safe to drink — Disinfecting with household bleach");

  await page.goto("/" + uri("lbn:org.lastbook.sample.docs/water-guide#sec=bleach"));
  const frame = page.frameLocator('[data-testid="html-view"]');
  await expect(frame.locator("#bleach.lbn-target")).toBeVisible();
  expect(await page.getByTestId("html-view").getAttribute("sandbox")).toBe("allow-same-origin");

  await page.goto("/" + uri("lbn:org.lastbook.sample.docs/supplies-checklist#page=2"));
  await expect(page.locator('[data-testid="pdf-canvas"][data-page="2"]')).toBeVisible();
  await expect(page.getByTestId("pdf-text")).toContainText("wrench", { useInnerText: false });

  await page.goto("/" + uri("lbn:org.lastbook.sample.video/storing-water#t=27"));
  const v = page.getByTestId("video");
  await expect.poll(() => v.evaluate((e: HTMLVideoElement) => [e.readyState >= 1, Math.round(e.currentTime)])).toEqual([true, 27]);
  await v.evaluate((e: HTMLVideoElement) => { e.muted = true; return e.play(); });
  await expect.poll(() => v.evaluate((e: HTMLVideoElement) => e.currentTime > 27.5)).toBe(true);
  await expect(page.locator('section[aria-label="Transcript"] li')).toHaveCount(6);
  await page.getByRole("button", { name: "Play from 0:13" }).click();
  await expect.poll(() => v.evaluate((e: HTMLVideoElement) => Math.floor(e.currentTime))).toBeGreaterThanOrEqual(13);

  await page.goto("/" + uri("lbn:org.lastbook.sample.map/ithaca#at=42.4396,-76.4970,15"));
  await expect(page.locator('[data-testid="map"][data-loaded="true"]')).toBeVisible({ timeout: 30_000 });

  await page.goto("/" + uri("lbn:org.lastbook.sample.encyclopedia/boiling-point#sec=altitude"));
  await expect(page.frameLocator('[data-testid="html-view"]').locator("#altitude.lbn-target")).toBeVisible();

  // A citation to something not installed explains itself instead of failing silently.
  await page.goto("/" + uri("lbn:org.example.missing/thing#page=2"));
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("This source cannot be opened");
});
