import { expect, type Locator, type Page } from "@playwright/test";
import type { BrowserFixtureId } from "../../src/testing/browser-fixtures";

export const stageObject = (page: Page, id: string): Locator =>
  page.locator(`[data-stage-object="${id}"]`);

export const jack = (page: Page, key: string): Locator => page.locator(`[data-jack="${key}"]`);

export async function openFixture(page: Page, fixture: BrowserFixtureId): Promise<void> {
  await page.goto(`/__test__/studio/${fixture}`);
  await expect(page.getByTestId("browser-fixture")).toHaveAttribute("data-fixture", fixture);
  // App's current DOM-anchor adapter measures after paint and repeats after the 450 ms face-flip
  // transition. These regressions deliberately exercise that existing adapter rather than an empty
  // pre-measurement state; Story 10.4 replaces this timeout-dependent seam.
  await page.waitForTimeout(550);
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
