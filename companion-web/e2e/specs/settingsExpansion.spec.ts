import { expect, test } from "@playwright/test";
import { injectFakeGeolocation } from "../fixtures/injectFakeGeolocation.js";

/**
 * Spec lines 128-145 — the Activity settings (prevent-screen-off, allow-GPS-
 * in-background, audio-cues, lock-screen live activity) must appear at the
 * top of the settings page, in spec order, with the cues + live-activity
 * toggles disabled until background-GPS is on.
 */
test.describe("Settings expansion — top-of-page toggles + gating", () => {
  test.beforeEach(async ({ page }) => {
    await injectFakeGeolocation(page);
  });

  test("renders the four activity toggles in spec order at the top", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => window.localStorage.clear());
    await page.reload();
    await page.getByRole("button", { name: "Settings" }).click();
    const section = page.locator("[data-testid='activity-settings']");
    await expect(section).toBeVisible();

    const ids = await section
      .locator("[data-testid^='setting-']")
      .evaluateAll((nodes) => nodes.map((n) => n.getAttribute("data-testid")));
    expect(ids).toEqual([
      "setting-keepScreenOn",
      "setting-allowBackgroundGps",
      "setting-audioCuesEnabled",
      "setting-liveActivityEnabled",
    ]);
  });

  test("audio cues + live activity are disabled until background GPS is on", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => window.localStorage.clear());
    await page.reload();
    await page.getByRole("button", { name: "Settings" }).click();

    const cuesCheckbox = page.locator(
      "[data-testid='setting-audioCuesEnabled'] input[type='checkbox']",
    );
    const liveCheckbox = page.locator(
      "[data-testid='setting-liveActivityEnabled'] input[type='checkbox']",
    );

    await expect(cuesCheckbox).toBeDisabled();
    await expect(liveCheckbox).toBeDisabled();

    const cuesRow = page.locator("[data-testid='setting-audioCuesEnabled']");
    await expect(cuesRow).toContainText(/Requires GPS/i);
    const liveRow = page.locator("[data-testid='setting-liveActivityEnabled']");
    await expect(liveRow).toContainText(/Requires GPS/i);

    await page
      .locator("[data-testid='setting-allowBackgroundGps'] input[type='checkbox']")
      .check();

    await expect(cuesCheckbox).toBeEnabled();
    await expect(liveCheckbox).toBeEnabled();
    // Default for cues is on, so the checkbox should reflect that.
    await expect(cuesCheckbox).toBeChecked();
  });

  test("the four toggles persist across page reload", async ({ page }) => {
    await page.goto("/");
    await page.evaluate(() => window.localStorage.clear());
    await page.reload();
    await page.getByRole("button", { name: "Settings" }).click();
    await page
      .locator("[data-testid='setting-keepScreenOn'] input[type='checkbox']")
      .check();
    await page
      .locator("[data-testid='setting-allowBackgroundGps'] input[type='checkbox']")
      .check();
    await page
      .locator("[data-testid='setting-liveActivityEnabled'] input[type='checkbox']")
      .check();

    await page.reload();
    await page.getByRole("button", { name: "Settings" }).click();

    await expect(
      page.locator("[data-testid='setting-keepScreenOn'] input[type='checkbox']"),
    ).toBeChecked();
    await expect(
      page.locator("[data-testid='setting-allowBackgroundGps'] input[type='checkbox']"),
    ).toBeChecked();
    await expect(
      page.locator("[data-testid='setting-liveActivityEnabled'] input[type='checkbox']"),
    ).toBeChecked();
  });
});
