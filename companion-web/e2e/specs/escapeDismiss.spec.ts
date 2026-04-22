import { expect, test } from "@playwright/test";
import { injectFakeGeolocation } from "../fixtures/injectFakeGeolocation.js";

/**
 * Flow #34 — Escape key dismisses the search panel.
 */
test.describe("Escape dismisses dropdown (L3, web-specific)", () => {
  test.beforeEach(async ({ page }) => {
    await injectFakeGeolocation(page);
  });

  test("pressing Escape closes the search panel", async ({ page }) => {
    await page.goto("/");
    const input = page.getByPlaceholder(/where to/i);
    await input.click();
    await input.fill("hel");
    const panel = page.locator("[data-testid='search-panel']");
    await expect(panel).toBeVisible({ timeout: 5000 });
    await page.keyboard.press("Escape");
    await expect(panel).toBeHidden({ timeout: 2000 });
  });
});
