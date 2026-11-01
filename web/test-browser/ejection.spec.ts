// Story 10.5.4 — approved ejection policy U3 through the real studio controls: eject and rack removal
// place gear at the nearest legal free spot to where it visibly was (never a stale stored position).
import { expect, test } from "@playwright/test";
import { objectChrome, openFixture, stageObject } from "./support/fixture-page";

test("eject stands the occupant free just above its retained rack, not at a stale position", async ({
  page,
}) => {
  await openFixture(page, "rack-hover");
  const rack = stageObject(page, "fixture-rack");
  const occupant = stageObject(page, "fixture-compressor");

  await occupant.hover();
  await expect(objectChrome(page, "fixture-compressor")).toHaveClass(/open/);
  await objectChrome(page, "fixture-compressor")
    .getByRole("button", { name: "eject from rack" })
    .click();

  // Mounted at slot 2 of the frame at x = 900: origin (914, 102.9). The retained 510.6 × 383.6 frame
  // blocks, so the nearest lattice spot is above it at (900, 400) — the stale stored x = 0 is ignored.
  await expect(
    objectChrome(page, "fixture-compressor").getByRole("button", { name: "back", exact: true }),
  ).toHaveCount(1); // free-standing now: it flips on its own instead of ejecting
  await expect(occupant).toHaveAttribute("style", /left: 900px; bottom: 400px;/);
  const rackBox = await rack.boundingBox();
  const box = await occupant.boundingBox();
  if (!rackBox || !box) throw new Error("rack or occupant vanished");
  expect(box.y + box.height).toBeLessThanOrEqual(rackBox.y + 1); // sits on top of the frame
});

test("removing a rack leaves its occupant standing where it was", async ({ page }) => {
  await openFixture(page, "rack-hover");
  const rack = stageObject(page, "fixture-rack");
  const occupant = stageObject(page, "fixture-compressor");
  const rackBox = await rack.boundingBox();
  if (!rackBox) throw new Error("fixture did not render");

  await page.mouse.move(rackBox.x + 2, rackBox.y + 2);
  await expect(objectChrome(page, "fixture-rack")).toHaveClass(/open/);
  await objectChrome(page, "fixture-rack").getByRole("button", { name: "remove rack" }).click();

  await expect(rack).toHaveCount(0);
  // The resolved world rect (the camera may refit, so assert world mm, not pixels): origin
  // (914, 102.9) with the frame gone snaps to the lattice point (900, 100).
  await expect(occupant).toHaveAttribute("style", /left: 900px; bottom: 100px;/);
});
