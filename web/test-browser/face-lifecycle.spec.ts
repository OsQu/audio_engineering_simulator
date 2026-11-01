import { expect, type Page, test } from "@playwright/test";
import { objectChrome, openFixture, stageObject } from "./support/fixture-page";

// Story 10.4: a flip is settled by its own CSS transition's generation, never by a timer. While it runs
// the face keeps its settled side, is non-patchable, and every lead touching it is suppressed.

const faceSlot = (page: Page, id: string) => page.locator(`[data-face-instance="studio:${id}"]`);
const leads = (page: Page) => page.locator("path.cable-core");

async function flip(page: Page, id: string, label: "front" | "back"): Promise<void> {
  await stageObject(page, id).hover();
  await objectChrome(page, id).getByRole("button", { name: label, exact: true }).click();
}

test("a flip suppresses its leads until its transition settles", async ({ page }) => {
  await openFixture(page, "cable-hover");
  await expect(leads(page)).toHaveCount(2);

  await flip(page, "rear-source", "front");

  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-state", "transitioning");
  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-interaction", "disabled");
  await expect(leads(page)).toHaveCount(1);

  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-state", "presented");
  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-side", "front");
  await expect(leads(page)).toHaveCount(2);
});

test("a rapid reversal settles on the latest generation only", async ({ page }) => {
  await openFixture(page, "cable-hover");

  await flip(page, "rear-source", "front");
  await objectChrome(page, "rear-source")
    .getByRole("button", { name: "back", exact: true })
    .click();

  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-state", "presented");
  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-side", "back");
  await expect(leads(page)).toHaveCount(2);
});

test("reduced motion settles a flip without waiting for an animation", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await openFixture(page, "cable-hover");

  await flip(page, "front-source", "back");

  await expect(faceSlot(page, "front-source")).toHaveAttribute("data-face-state", "presented", {
    timeout: 100,
  });
  await expect(faceSlot(page, "front-source")).toHaveAttribute("data-face-side", "back");
  await expect(leads(page)).toHaveCount(2);
});

test("a rack flip transitions its mounted occupant and settles it to the rack's side", async ({
  page,
}) => {
  await openFixture(page, "rack-hover");
  const occupant = faceSlot(page, "fixture-compressor");
  await expect(occupant).toHaveAttribute("data-flip-owner", "fixture-rack");

  const rack = await stageObject(page, "fixture-rack").boundingBox();
  if (!rack) throw new Error("fixture rack did not render");
  await page.mouse.move(rack.x + 2, rack.y + 2);
  await page.getByRole("button", { name: "turn rack around" }).click();

  await expect(occupant).toHaveAttribute("data-face-state", "transitioning");
  await expect(occupant).toHaveAttribute("data-face-side", "front");
  await expect(occupant).toHaveAttribute("data-face-state", "presented");
  await expect(occupant).toHaveAttribute("data-face-side", "back");
});

test("leaving the wall mid-flip invalidates it; returning shows the settled durable side", async ({
  page,
}) => {
  await openFixture(page, "cable-hover");

  await flip(page, "rear-source", "front");
  await page.getByRole("button", { name: "Right", exact: true }).click();
  await page.getByRole("button", { name: "Front", exact: true }).click();

  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-state", "presented");
  await expect(faceSlot(page, "rear-source")).toHaveAttribute("data-face-side", "front");
  await expect(leads(page)).toHaveCount(2);
});

test("the workbench DUT exposes independent front and back slots", async ({ page }) => {
  await page.goto("/devices/synth_voice");
  await expect(page.locator('[data-face-instance="workbench:dut-front:dev"]')).toHaveCount(1, {
    timeout: 15000,
  });
  await expect(page.locator('[data-face-instance="workbench:dut-back:dev"]')).toHaveCount(1);
});

test("a focus-window copy cannot start a patch from its duplicate jack", async ({ page }) => {
  await page.goto("/devices/synth_voice");
  await expect(page.locator('[data-geometry="current"]').first()).toBeAttached({ timeout: 15000 });
  await page.locator('[data-face-instance="workbench:dut-front:dev"]').hover();
  await page.getByRole("button", { name: /^open / }).click();

  const focus = page.locator('[data-face-instance="focus:bench:dev"]');
  await expect(focus).toHaveAttribute("data-interaction", "disabled");
  // The synth's sockets sit on its back face, which a focus copy never turns toward the operator, so
  // the press is dispatched onto the copy's jack element directly (the stage socket of the same key is
  // measured and would otherwise lend it an anchor).
  const copy = focus.locator('[data-jack="dev:output:0"]');
  await expect(copy).toHaveCount(1);
  await copy.dispatchEvent("pointerdown", { bubbles: true, button: 0, buttons: 1, pointerId: 1 });
  await page.mouse.move(200, 200, { steps: 3 });
  await expect(page.locator("path.cable-drag")).toHaveCount(0);
});
