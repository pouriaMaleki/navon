import { expect, test } from "@playwright/test";
import { injectFakeGeolocation } from "../fixtures/injectFakeGeolocation.js";

/**
 * Flow #46 — long-press the map to drop a destination pin and trigger a route.
 *
 * Expected RED on web: MapLibre long-press-to-destination is not wired today.
 * The plan explicitly calls this out in the "Expected RED flows" section.
 */
test.describe("long-press drops destination (L3)", () => {
  test.beforeEach(async ({ page }) => {
    await injectFakeGeolocation(page);
  });

  test("holding pointer on map surface for ~600ms routes to that location", async ({ page }) => {
    await page.goto("/");
    const viewport = page.viewportSize();
    if (!viewport) throw new Error("viewport not available");
    const x = viewport.width / 2;
    const y = viewport.height / 2;

    await page.mouse.move(x, y);
    await page.mouse.down();
    // Hold long enough for a long-press handler to fire.
    await page.waitForTimeout(700);
    await page.mouse.up();

    // A long-press should produce a destination preview: either the context
    // menu or a new route plan. We assert the most visible outcome — the
    // "Where to?" input receives a non-empty value. RED until wired.
    const input = page.getByPlaceholder(/where to/i);
    await expect(input).not.toHaveValue("", { timeout: 5000 });
  });
});
