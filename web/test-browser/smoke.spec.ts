import { expect, test } from "@playwright/test";

test("rack fixture mounts the real studio tree without audio", async ({ page }) => {
  await page.goto("/__test__/studio/rack-hover");

  await expect(page.getByTestId("browser-fixture")).toHaveAttribute("data-fixture", "rack-hover");
  await expect(page.getByRole("button", { name: "fixture-rack — drag to move" })).toBeVisible();
  await expect(
    page.getByRole("button", { name: "fixture-compressor — drag to move" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: /start/i })).toBeEnabled();
});
