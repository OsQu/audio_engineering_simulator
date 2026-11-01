import { expect, type Locator, type Page } from "@playwright/test";
import type { BrowserFixtureId } from "../../src/testing/browser-fixtures";

export const stageObject = (page: Page, id: string): Locator =>
  page.locator(`[data-stage-object="${id}"]`);

/** An object's toolbar in the interaction layer (Story 10.5): keyed by object ID, not nested in it. */
export const objectChrome = (page: Page, id: string): Locator =>
  page.locator(`[data-chrome-for="${id}"]`);

export const jack = (page: Page, key: string): Locator => page.locator(`[data-jack="${key}"]`);

export async function openFixture(page: Page, fixture: BrowserFixtureId): Promise<void> {
  await page.goto(`/__test__/studio/${fixture}`);
  await expect(page.getByTestId("browser-fixture")).toHaveAttribute("data-fixture", fixture);
  await geometrySettled(page);
  await cameraSettled(page);
}

/** The auto-fit camera has stopped moving: a late refit would slide gear out from under a stationary
 *  pointer, firing boundary events that re-target hover (and the open toolbar) mid-test. */
export async function cameraSettled(page: Page): Promise<void> {
  const transform = () =>
    page.evaluate(
      () => (document.querySelector(".surface") as HTMLElement | null)?.style.transform,
    );
  let previous = await transform();
  await expect
    .poll(
      async () => {
        const current = await transform();
        const stable = current === previous;
        previous = current;
        return stable;
      },
      { intervals: [100] },
    )
    .toBe(true);
}

export async function elementCentre(locator: Locator): Promise<{ x: number; y: number }> {
  const box = await locator.boundingBox();
  if (!box) throw new Error("expected a visible element with a bounding box");
  return { x: box.x + box.width / 2, y: box.y + box.height / 2 };
}

export async function topmostDeviceAt(
  page: Page,
  point: { x: number; y: number },
): Promise<string | null> {
  return page.evaluate(
    ({ x, y }) =>
      document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-stage-object]")?.dataset
        .stageObject ?? null,
    point,
  );
}

export async function renderedLayer(locator: Locator): Promise<string> {
  return locator.evaluate((element) => getComputedStyle(element).zIndex);
}

/** Every stage face slot has registered current socket geometry (Story 10.4) — an event, not a timer. */
export async function geometrySettled(page: Page): Promise<void> {
  await expect(page.locator('[data-geometry="pending"]')).toHaveCount(0);
  await expect(page.locator("[data-geometry]").first()).toBeAttached();
}
