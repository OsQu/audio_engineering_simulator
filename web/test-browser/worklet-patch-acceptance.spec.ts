import { expect, test } from "@playwright/test";

test("generated worklet accepts then installs a compatible topology transaction", async ({
  page,
}) => {
  await page.goto("/__test__/worklet/patch-acceptance");

  const state = page.getByTestId("worklet-state");
  await expect(state).toHaveAttribute("data-state", "idle");
  await page.getByRole("button", { name: "Start real worklet" }).click();
  await expect(state).toHaveAttribute("data-state", "ready", { timeout: 30_000 });

  await page.getByRole("button", { name: "Submit compatible patch" }).click();
  await expect(state).toHaveAttribute("data-state", "installed", { timeout: 30_000 });
  await expect(state).toHaveAttribute("data-events", "accepted,installed");
});

test("generated worklet rejects an invalid topology transaction without installing it", async ({
  page,
}) => {
  await page.goto("/__test__/worklet/patch-acceptance");
  const state = page.getByTestId("worklet-state");
  await page.getByRole("button", { name: "Start real worklet" }).click();
  await expect(state).toHaveAttribute("data-state", "ready", { timeout: 30_000 });

  await page.getByRole("button", { name: "Submit rejected patch" }).click();
  await expect(state).toHaveAttribute("data-state", "rejected", { timeout: 30_000 });
  await expect(state).toHaveAttribute("data-events", "");
});

test("a protocol mismatch fails closed before worklet readiness", async ({ page }) => {
  await page.goto("/__test__/worklet/patch-acceptance?mismatch=1");
  const state = page.getByTestId("worklet-state");
  await page.getByRole("button", { name: "Start real worklet" }).click();
  await expect(state).toHaveAttribute("data-state", "starting");
  await expect(page.getByTestId("worklet-detail")).toContainText("topology protocol mismatch", {
    timeout: 30_000,
  });
});
