import { test, expect, webkit } from "@playwright/test";

// Unsupported-device state: a WebKit private/ephemeral window has no usable file storage. The app must say so at once.
test("a window without storage explains itself", async ({ browserName }) => {
  test.skip(browserName !== "webkit", "only WebKit ephemeral windows lack storage here");
  const b = await webkit.launch();
  const page = await b.newPage();
  await page.goto("http://127.0.0.1:18787/");
  await expect(page.getByTestId("no-storage")).toContainText("cannot store a library");
  await b.close();
});
