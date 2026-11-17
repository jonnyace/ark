import { expect } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";
import { test, installFromAddress, uri } from "./helpers";

const views = [
  ["home", "/#/"], ["library", "/#/library"], ["search", "/#/search?q=water"], ["packs", "/#/packs"], ["ai", "/#/ai"],
  ["saved", "/#/saved"], ["about", "/#/about"], ["ask", "/#/ask"],
  ["html", "/" + uri("lbn:org.lastbook.sample.docs/water-guide#sec=boil")],
  ["pdf", "/" + uri("lbn:org.lastbook.sample.docs/supplies-checklist#page=1")],
  ["video", "/" + uri("lbn:org.lastbook.sample.video/storing-water#t=5")],
];

// Automated WCAG 2 A/AA checks (axe-core) on every view, plus a narrow phone-width layout check.
// Automated checks find only part of the accessibility problems; manual screen-reader testing is still needed.
test("views pass automated accessibility checks and fit a 360 px screen", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "one engine is enough for axe; layout is checked there too");
  test.setTimeout(240_000);
  for (const p of ["org.lastbook.sample.docs", "org.lastbook.sample.video"]) {
    await installFromAddress(page, `http://127.0.0.1:18787/packs/${p}/pack.json`);
    await expect(page.locator(`[data-testid="pack"][data-pack="${p}"]`)).toBeVisible({ timeout: 60_000 });
  }
  const problems: string[] = [];
  for (const [name, url] of views) {
    await page.setViewportSize({ width: 1100, height: 900 });
    await page.goto(url);
    await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
    await page.waitForTimeout(800);
    const r = await new AxeBuilder({ page }).withTags(["wcag2a", "wcag2aa"]).exclude("iframe").analyze();
    for (const v of r.violations) problems.push(`${name}: ${v.id} (${v.impact}) ${v.nodes.length}× — ${v.nodes[0]?.target.join(" ")}`);
    await page.setViewportSize({ width: 360, height: 740 });
    await page.waitForTimeout(300);
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    if (overflow > 1) problems.push(`${name}: page is ${overflow}px wider than a 360px screen`);
  }
  console.log(problems.join("\n") || "no automated accessibility or layout problems found");
  expect(problems).toEqual([]);
});

test("keyboard: skip link reaches the content", async ({ page, browserName }) => {
  test.skip(browserName !== "chromium", "keyboard behaviour checked once");
  await page.goto("/#/");
  await expect(page.getByRole("heading", { level: 1 })).toBeVisible();
  await page.keyboard.press("Tab");
  await expect(page.getByRole("link", { name: "Skip to content" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.locator("main")).toBeFocused();
});
