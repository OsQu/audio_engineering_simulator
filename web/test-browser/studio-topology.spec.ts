import { expect, test } from "@playwright/test";

// The real SceneSession → worklet path for a structural edit. The studio fixtures replace
// `proposeTopology` and the worklet smoke test posts hand-built metadata, so neither caught the
// DataCloneError when the session posted a request still carrying its edit closure.
test("adding gear in the studio commits through the real worklet", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", (error) => errors.push(error.message));
  await page.addInitScript(() => localStorage.clear());
  await page.goto("/");
  await page.getByRole("button", { name: /start/i }).click();
  await expect(page.locator("summary", { hasText: "+ Add" })).toBeVisible({ timeout: 15000 });

  const before = await page.locator("[data-stage-object]").count();
  await page.locator("summary", { hasText: "+ Add" }).click();
  await page.locator(".add-chip").first().click();

  await expect(page.locator("[data-stage-object]")).toHaveCount(before + 1, { timeout: 10000 });
  expect(errors).toEqual([]);
});
