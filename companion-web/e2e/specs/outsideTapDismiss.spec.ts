import { expect, test } from "@playwright/test";
import { injectFakeGeolocation } from "../fixtures/injectFakeGeolocation.js";

/**
 * Flow #33 — tapping outside the search panel dismisses the dropdown.
 */
test.describe("outside tap dismisses dropdown (L3)", () => {
  test.beforeEach(async ({ page }) => {
    await injectFakeGeolocation(page);
  });

  test("map area tap closes the open search panel", async ({ page }) => {
    await page.goto("/");
    const input = page.getByPlaceholder(/where to/i);
    await input.click();
    await input.fill("hel");
    const panel = page.locator("[data-testid='search-panel']");
    await expect(panel).toBeVisible({ timeout: 5000 });
    // Click the map area — far from the panel.
    const viewport = page.viewportSize();
    if (!viewport) throw new Error("viewport not available");
    await page.mouse.click(viewport.width / 2, viewport.height - 50);
    await expect(panel).toBeHidden({ timeout: 2000 });
  });
});
