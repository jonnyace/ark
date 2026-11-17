import { expect } from "@playwright/test";
import { test } from "./helpers";

// Browser storage is never presented as permanent: the quota is shown, persistence is requested only when the person
// asks, and what the page says matches what the browser actually reports (navigator.storage.persisted()).
test("storage status reflects the browser's real persistence answer", async ({ page }) => {
  await page.goto("/#/packs");
  const card = page.getByRole("region", { name: "Storage" });
  await expect(card).toContainText("browser quota:");
  await expect(card).toContainText(/Not persistent|Persistent storage granted/);
  const ask = page.getByRole("button", { name: "Ask the browser to keep my library" });
  if (await ask.isVisible()) await ask.click();
  await page.waitForTimeout(500);
  const persisted = await page.evaluate(() => navigator.storage.persisted());
  await page.goto("/#/packs");
  if (persisted) {
    await expect(card).toContainText("Persistent storage granted");
    await expect(card).toContainText("Clearing site data still deletes it");
  } else {
    await expect(card).toContainText("Not persistent: the browser may clear this library");
  }
  for (const v of ["/#/", "/#/packs", "/#/about"]) {
    await page.goto(v);
    await expect(page.locator("main")).not.toContainText(/(?<!not )guaranteed (permanent|forever)|never be (lost|deleted)|stored permanently/i);
  }
  console.log(`persisted after request: ${persisted}`);
});

// The "granted" path, observed for real: Firefox with its persistent-storage permission set to "allow".
test("when the browser grants persistence, the page says so without promising permanence", async ({ browserName }) => {
  test.skip(browserName !== "firefox", "uses a Firefox permission preference");
  const { firefox } = await import("@playwright/test");
  const b = await firefox.launch({ firefoxUserPrefs: { "permissions.default.persistent-storage": 1 } });
  const page = await b.newPage();
  await page.goto("http://127.0.0.1:18787/#/packs");
  await page.getByRole("button", { name: "Ask the browser to keep my library" }).click();
  await expect.poll(() => page.evaluate(() => navigator.storage.persisted())).toBe(true);
  const card = page.getByRole("region", { name: "Storage" });
  await expect(card).toContainText("Persistent storage granted");
  await expect(card).toContainText("Clearing site data still deletes it");
  await expect(page.getByRole("button", { name: "Ask the browser to keep my library" })).toHaveCount(0);
  await b.close();
});
