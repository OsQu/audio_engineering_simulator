import { expect, test } from "@playwright/test";
import { openFixture, stageObject } from "./support/fixture-page";

// Graduated from the known-regression suite by Story 10.3: App owns the rack preview, so every mounted
// occupant resolves through the previewed rack before the drag commits.
test("populated rack drag moves the occupant before release", async ({ page }) => {
  await openFixture(page, "rack-drag");

  const rack = stageObject(page, "fixture-rack");
  const occupant = stageObject(page, "fixture-compressor");
  const rackBox = await rack.boundingBox();
  const before = await occupant.boundingBox();
  if (!rackBox || !before) throw new Error("rack-drag fixture did not render");

  await page.mouse.move(rackBox.x + 2, rackBox.y + 2);
  await page.mouse.down();
  await page.mouse.move(rackBox.x + 160, rackBox.y + 40);
  const during = await occupant.boundingBox();
  await page.mouse.up();

  expect(during?.x).toBeGreaterThan(before.x + 100);
});
