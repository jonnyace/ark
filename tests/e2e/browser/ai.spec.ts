import { test, expect } from "@playwright/test";
import { installFromAddress } from "./helpers";

// Criterion 6: real in-browser inference (wllama, CPU, this Chromium), grounded with citations; measured, not mocked.
test("browser local AI answers from the library with citations", async ({ page }) => {
  test.setTimeout(900_000);
  for (const p of ["org.lastbook.sample.docs", "org.lastbook.model.qwen2.5-1.5b-instruct"]) {
    await installFromAddress(page, `http://127.0.0.1:18787/packs/${p}/pack.json`);
    await expect(page.locator(`[data-testid="pack"][data-pack="${p}"]`)).toBeVisible({ timeout: 300_000 });
  }
  await page.goto("/#/ai");
  await expect(page.getByText(/Runtime available: wllama/)).toBeVisible({ timeout: 30_000 });
  await page.getByLabel("Use local AI").check();
  await page.goto("/#/ask");
  await page.getByLabel("Your question").fill("How many drops of 6 percent bleach should I add to a gallon of water?");
  const t0 = Date.now();
  await page.getByRole("button", { name: "Ask", exact: true }).click();
  await expect(page.getByTestId("answer")).toHaveAttribute("data-outcome", "answered", { timeout: 600_000 });
  console.log(`browser answer in ${((Date.now() - t0) / 1000).toFixed(1)} s: ${await page.getByTestId("answer-text").textContent()}`);
  await expect(page.getByTestId("answer-text")).toContainText(/8 drops|eight drops/);
  await page.getByTestId("cite").first().click();
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Making water safe to drink");
});
