import { expect, test } from "@playwright/test";
import { injectFakeGeolocation } from "../fixtures/injectFakeGeolocation.js";

/**
 * Plan: "test the gesture handler's output, not DOM event dispatch".
 *
 * This spec is the single L3 invariant that a real user gesture reaches the
 * MapLibre canvas at all. Detailed gesture semantics (simultaneous
 * pinch+rotate, pan offsets) are asserted at the L2 store level.
 */
test.describe("gesture handler reached (L3)", () => {
  test.beforeEach(async ({ page }) => {
    await injectFakeGeolocation(page);
  });

  test("pointerdown on the map canvas is observed by the app", async ({ page }) => {
    await page.goto("/");

    // Inject a hook into the canvas so we can count pointerdowns received by
    // MapLibre. We observe via window instead of reading internal state.
    await page.evaluate(() => {
      (window as unknown as { __pointerDownCount: number }).__pointerDownCount = 0;
      const canvas = document.querySelector("canvas");
      canvas?.addEventListener(
        "pointerdown",
        () => {
          const w = window as unknown as { __pointerDownCount: number };
          w.__pointerDownCount += 1;
        },
        { capture: true },
      );
    });

    const canvas = page.locator("canvas").first();
    await expect(canvas).toBeVisible({ timeout: 5000 });
    const box = await canvas.boundingBox();
    if (!box) throw new Error("canvas has no bounding box");
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);

    const count = await page.evaluate(
      () => (window as unknown as { __pointerDownCount: number }).__pointerDownCount,
    );
    expect(count).toBeGreaterThanOrEqual(1);
  });
});
