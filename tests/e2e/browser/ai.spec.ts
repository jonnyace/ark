import { expect } from "@playwright/test";
import { test, installFromAddress } from "./helpers";

test.use({ persistent: true }); // the model is 1.1 GB: store it on disk like a real browser profile

// Criterion 6: real in-browser inference (wllama, CPU, this Chromium), grounded with citations; measured, not mocked.
test("browser local AI answers from the library with citations", async ({ page, browserName }) => {
  test.skip(browserName === "webkit", "WebKit's 1000 MB quota here is below the 1.5B model; see the low-space and small-model tests");
  // Installing the 1.1 GB model into browser storage (download + hashing in WebAssembly) takes ~7 s on the development
  // machine but several minutes on a 4-core hosted CI runner.
  test.setTimeout(1_500_000);
  for (const p of ["org.lastbook.sample.docs", "org.lastbook.model.qwen2.5-1.5b-instruct"]) {
    await installFromAddress(page, `http://127.0.0.1:18787/packs/${p}/pack.json`);
    try {
      await expect(page.locator(`[data-testid="pack"][data-pack="${p}"]`)).toBeVisible({ timeout: 600_000 });
    } catch (e) {
      console.log(`install of ${p} did not finish; jobs: ${await page.locator('[data-testid="job"]').allTextContents()}`);
      console.log(`storage: ${await page.getByRole("region", { name: "Storage" }).textContent().catch(() => "?")}`);
      throw e;
    }
  }
  await page.goto("/#/ai");
  await expect(page.getByText(/Runtime available: wllama/)).toBeVisible({ timeout: 30_000 });
  await page.getByLabel("Use local AI").check();
  await page.goto("/#/ask");
  await page.getByLabel("Your question").fill("How many drops of 6 percent bleach should I add to a gallon of water?");
  const t0 = Date.now();
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(page.getByTestId("answer")).toHaveAttribute("data-outcome", "answered", { timeout: 600_000 });
  console.log(`${browserName}: browser answer in ${((Date.now() - t0) / 1000).toFixed(1)} s: ${await page.getByTestId("answer-text").textContent()}`);
  await expect(page.getByTestId("answer-text")).toContainText(/8 drops|eight drops/);
  await page.getByTestId("cite").first().click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Making water safe to drink");
});

// Low-space state, measured: Playwright WebKit grants this origin 1000 MB, less than the 1.5B model pack needs.
test("a model larger than the browser quota is refused before downloading", async ({ page, browserName }) => {
  test.skip(browserName !== "webkit", "only WebKit has a quota this small here");
  await installFromAddress(page, "http://127.0.0.1:18787/packs/org.lastbook.model.qwen2.5-1.5b-instruct/pack.json");
  await expect(page.locator('[data-testid="job"][data-state="failed"]')).toContainText("not enough browser storage");
  await expect(page.locator('[data-testid="pack"]')).toHaveCount(0);
});

// Feasibility in WebKit with the small model (not recommended for real use: see TESTING.md for its wrong answers).
test("the small model runs in WebKit", async ({ page, browserName }) => {
  test.skip(browserName !== "webkit", "WebKit-specific feasibility check");
  test.setTimeout(900_000);
  for (const p of ["org.lastbook.sample.docs", "org.lastbook.model.qwen2.5-0.5b-instruct"]) {
    await installFromAddress(page, `http://127.0.0.1:18787/packs/${p}/pack.json`);
    await expect(page.locator(`[data-testid="pack"][data-pack="${p}"]`)).toBeVisible({ timeout: 300_000 });
  }
  await page.goto("/#/ai");
  await page.getByLabel("Use local AI").check();
  await page.goto("/#/ask");
  await page.getByLabel("Your question").fill("How often should I replace stored tap water?");
  const t0 = Date.now();
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(page.getByTestId("answer")).toHaveAttribute("data-outcome", "answered", { timeout: 600_000 });
  console.log(`webkit (0.5B): answer in ${((Date.now() - t0) / 1000).toFixed(1)} s: ${await page.getByTestId("answer-text").textContent()}`);
});
