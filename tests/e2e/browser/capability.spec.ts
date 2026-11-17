import { expect } from "@playwright/test";
import { test } from "./helpers";

// Every browser must either offer local AI or say plainly why not; never a silent fallback to a remote service.
test("AI capability is reported honestly", async ({ page, browserName }) => {
  const remote: string[] = [];
  page.on("request", (r) => { if (!r.url().startsWith("http://127.0.0.1")) remote.push(r.url()); });
  await page.goto("/#/ai");
  const ok = page.getByText(/Runtime available: wllama/);
  const no = page.getByTestId("ai-unavailable");
  await expect(ok.or(no)).toBeVisible({ timeout: 30_000 });
  const msg = (await ok.isVisible()) ? await ok.textContent() : await no.textContent();
  console.log(`${browserName}: ${msg}`);
  if (await no.isVisible()) expect(msg).toMatch(/cannot run the local AI|lacks/);
  expect(remote).toEqual([]);
});
