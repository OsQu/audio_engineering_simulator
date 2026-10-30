import { expect, type Page, test } from "@playwright/test";
import { openFixture, stageObject } from "./support/fixture-page";

// Story 10.4: a flip is settled by its own CSS transition's generation, never by a timer. While it runs
// the face keeps its settled side, is non-patchable, and every lead touching it is suppressed.

const faceSlot = (page: Page, id: string) => page.locator(`[data-face-instance="studio:${id}"]`);
const leads = (page: Page) => page.locator("path.cable-core");

async function flip(page: Page, id: string, label: "front" | "back"): Promise<void> {
  await stageObject(page, id).hover();
  await stageObject(page, id).getByRole("button", { name: label, exact: true }).click();
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
  await stageObject(page, "rear-source").getByRole("button", { name: "back", exact: true }).click();

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
