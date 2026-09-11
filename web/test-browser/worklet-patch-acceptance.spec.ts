import { expect, test } from "@playwright/test";

test("generated worklet accepts a compatible patch", async ({ page }) => {
  await page.goto("/__test__/worklet/patch-acceptance");

  const state = page.getByTestId("worklet-state");
  await expect(state).toHaveAttribute("data-state", "idle");
  await page.getByRole("button", { name: "Start real worklet" }).click();
  await expect(state).toHaveAttribute("data-state", "ready", { timeout: 30_000 });

  await page.getByRole("button", { name: "Submit compatible patch" }).click();
  await expect(state).toHaveAttribute("data-state", "accepted", { timeout: 30_000 });
});
