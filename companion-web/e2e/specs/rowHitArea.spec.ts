import { expect, test } from "@playwright/test";
import { injectFakeGeolocation } from "../fixtures/injectFakeGeolocation.js";

/**
 * Flow #32 — row hit area covers full row width. Regression-lock for a bug
 * where the dropdown `<button>` defaulted to content-width so clicks in the
 * padding missed the handler.
 */
test.describe("where-to dropdown hit area (L3)", () => {
  test.beforeEach(async ({ page }) => {
    await injectFakeGeolocation(page);
  });

  test("tapping near the row edge selects the item", async ({ page }) => {
    await page.goto("/");
    const input = page.getByPlaceholder(/where to/i);
    await input.click();
    await input.fill("hel");
    const panel = page.locator("[data-testid='search-panel']");
    await expect(panel).toBeVisible({ timeout: 5000 });

    const row = page.locator("[data-testid='search-row']").first();
    await expect(row).toBeVisible({ timeout: 5000 });

    const box = await row.boundingBox();
    if (!box) throw new Error("row has no bounding box");
    await page.mouse.click(box.x + 10, box.y + box.height / 2);
    await expect(panel).toBeHidden({ timeout: 2000 });
  });
});
