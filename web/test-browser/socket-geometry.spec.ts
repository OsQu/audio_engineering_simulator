import { expect, type Locator, type Page, test } from "@playwright/test";
import { elementCentre, jack, openFixture, stageObject } from "./support/fixture-page";

// Story 10.4: socket geometry is registered once per face revision in face-local mm. Camera zoom and
// placement moves re-project it; they never re-measure, and the lead still lands on the socket.

const slot = (page: Page, id: string): Locator =>
  page.locator(`[data-face-instance="studio:${id}"]`);

/** The client-space start and end of the first rendered lead. */
async function leadEnds(
  page: Page,
): Promise<{ a: { x: number; y: number }; b: { x: number; y: number } }> {
  return page
    .locator("path.cable-core")
    .first()
    .evaluate((el) => {
      const path = el as SVGPathElement;
      const m = path.getScreenCTM();
      if (!m) throw new Error("lead has no screen transform");
      const at = (len: number) => {
        const p = path.getPointAtLength(len).matrixTransform(m);
        return { x: p.x, y: p.y };
      };
      return { a: at(0), b: at(path.getTotalLength()) };
    });
}

async function expectLeadOnSockets(page: Page, from: string, to: string): Promise<void> {
  const src = await elementCentre(jack(page, from));
  const dst = await elementCentre(jack(page, to));
  await expect
    .poll(async () => {
      const { a, b } = await leadEnds(page);
      return Math.max(Math.hypot(a.x - src.x, a.y - src.y), Math.hypot(b.x - dst.x, b.y - dst.y));
    })
    .toBeLessThan(2);
}

test("zoom and placement re-project registered geometry without remeasuring", async ({ page }) => {
  await openFixture(page, "cable-hover");
  await expectLeadOnSockets(page, "rear-source:output:0", "rear-sink:input:0");

  const before = await slot(page, "rear-source").getAttribute("data-geometry-generation");
  expect(before).not.toBeNull();

  // Camera: cursor-anchored wheel zoom.
  const centre = await elementCentre(stageObject(page, "rear-source"));
  await page.mouse.move(centre.x, centre.y);
  await page.mouse.wheel(0, -400);
  await expectLeadOnSockets(page, "rear-source:output:0", "rear-sink:input:0");

  // Placement: drag the source body along the wall.
  const box = await stageObject(page, "rear-source").boundingBox();
  if (!box) throw new Error("source did not render");
  await page.mouse.move(box.x + box.width / 2, box.y + 4);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 - 120, box.y + 4, { steps: 6 });
  await page.mouse.up();
  const moved = await stageObject(page, "rear-source").boundingBox();
  expect(Math.abs((moved?.x ?? box.x) - box.x)).toBeGreaterThan(50);
  await expectLeadOnSockets(page, "rear-source:output:0", "rear-sink:input:0");

  await expect(slot(page, "rear-source")).toHaveAttribute("data-geometry-generation", before ?? "");
});

test("the workbench projects its DUT sockets through zoom without remeasuring", async ({
  page,
}) => {
  await page.goto("/devices/synth_voice");
  const marker = page.locator("circle.tap-marker");
  await expect(marker).toBeAttached({ timeout: 15000 });
  await expect(page.locator('[data-geometry="pending"]')).toHaveCount(0);

  // The "listening here" marker is drawn at the projected anchor of the DUT's monitored output socket.
  const offset = async (): Promise<number> => {
    const at = await marker.evaluate((el) => {
      const c = el as SVGCircleElement;
      const m = c.getScreenCTM();
      if (!m) throw new Error("marker has no screen transform");
      const p = new DOMPoint(c.cx.baseVal.value, c.cy.baseVal.value).matrixTransform(m);
      return { x: p.x, y: p.y };
    });
    const socket = await elementCentre(jack(page, "dev:output:0").filter({ visible: true }));
    return Math.hypot(at.x - socket.x, at.y - socket.y);
  };
  const generations = () =>
    page
      .locator("[data-geometry-generation]")
      .evaluateAll((els) => els.map((e) => (e as HTMLElement).dataset.geometryGeneration));

  await expect.poll(offset).toBeLessThan(1);
  const before = await generations();

  const viewport = await page.locator(".viewport").boundingBox();
  if (!viewport) throw new Error("bench viewport did not render");
  await page.mouse.move(viewport.x + viewport.width / 3, viewport.y + viewport.height / 3);
  await page.mouse.wheel(0, -500);

  await expect.poll(offset).toBeLessThan(1);
  expect(await generations()).toEqual(before);
});
