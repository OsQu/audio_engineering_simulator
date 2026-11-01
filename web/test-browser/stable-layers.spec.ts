// Story 10.5.2: physical paint order is resolver-owned. Hover, keyboard focus, toolbar grace, and drag
// can never change which chassis is on top, and toolbars live in a separate interaction layer.
// Graduated from known-regressions.spec.ts (rack-hover, rear/front cable hover, populated rack drag).
import { expect, type Locator, type Page, test } from "@playwright/test";
import {
  elementCentre,
  openFixture,
  renderedLayer,
  stageObject,
  topmostDeviceAt,
} from "./support/fixture-page";

const TOOLBAR_GRACE_MS = 300; // WorldView's hideChromeSoon delay

const paintLayer = (locator: Locator): Promise<string | null> =>
  locator.getAttribute("data-paint-layer");

/** The stage-object id owning the element under `point`, or the chrome's owner when it is a toolbar. */
const topmostChromeAt = (page: Page, point: { x: number; y: number }): Promise<string | null> =>
  page.evaluate(
    ({ x, y }) =>
      document.elementFromPoint(x, y)?.closest<HTMLElement>("[data-chrome-for]")?.dataset
        .chromeFor ?? null,
    point,
  );

test("rack hover leaves a mounted device topmost", async ({ page }) => {
  await openFixture(page, "rack-hover");

  const rack = stageObject(page, "fixture-rack");
  const occupant = stageObject(page, "fixture-compressor");
  const rackBox = await rack.boundingBox();
  const occupantCentre = await elementCentre(occupant);
  if (!rackBox) throw new Error("fixture rack did not render");
  await page.mouse.move(rackBox.x + 2, rackBox.y + 2);

  await expect(page.locator('[data-chrome-for="fixture-rack"]')).toHaveClass(/open/);
  await expect.poll(() => topmostDeviceAt(page, occupantCentre)).toBe("fixture-compressor");
  expect(await paintLayer(rack)).toBe("furniture");
});

for (const { id, layer } of [
  { id: "rear-source", layer: "rear-object" },
  { id: "front-source", layer: "front-object" },
]) {
  test(`${layer} hover preserves its cable layer`, async ({ page }) => {
    await openFixture(page, "cable-hover");

    const device = stageObject(page, id);
    const before = await renderedLayer(device);
    await device.hover();
    await expect(page.locator(`[data-chrome-for="${id}"]`)).toHaveClass(/open/);

    expect(await paintLayer(device)).toBe(layer);
    expect(await renderedLayer(device)).toBe(before);
  });
}

test("hover, grace, focus, and drag never change rack/occupant occlusion", async ({ page }) => {
  await openFixture(page, "rack-hover");

  const rack = stageObject(page, "fixture-rack");
  const occupant = stageObject(page, "fixture-compressor");
  const rackBox = await rack.boundingBox();
  if (!rackBox) throw new Error("fixture rack did not render");
  const occupantOnTop = async (): Promise<void> => {
    const centre = await elementCentre(occupant);
    expect(await topmostDeviceAt(page, centre)).toBe("fixture-compressor");
    expect(await renderedLayer(rack)).toBe("0");
  };

  await page.mouse.move(rackBox.x + 2, rackBox.y + 2); // hover
  await occupantOnTop();
  await page.mouse.move(5, 5); // leave: inside the toolbar grace period
  await occupantOnTop();
  await page.waitForTimeout(TOOLBAR_GRACE_MS + 50); // grace elapsed (asserting no late raise)
  await occupantOnTop();
  await rack.focus(); // keyboard focus opens the rack's toolbar
  await expect(page.locator('[data-chrome-for="fixture-rack"]')).toHaveClass(/open/);
  await occupantOnTop();

  await page.mouse.move(rackBox.x + 2, rackBox.y + 2);
  await page.mouse.down();
  await page.mouse.move(rackBox.x + 80, rackBox.y + 20);
  await expect(page.locator('[data-drag-outline="fixture-rack"]')).toBeVisible();
  await occupantOnTop(); // mid-drag: the assembly moves, the frame stays behind its occupant
  await page.mouse.up();
  await occupantOnTop();
});

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

test("an occupant's toolbar controls are hittable above the rack and its neighbours", async ({
  page,
}) => {
  await openFixture(page, "rack-hover");

  const occupant = stageObject(page, "fixture-compressor");
  await occupant.hover();
  const toolbar = page.locator('[data-chrome-for="fixture-compressor"]');
  await expect(toolbar).toHaveClass(/open/);
  const controls = toolbar.locator("button, select");
  const count = await controls.count();
  expect(count).toBeGreaterThan(0);
  // Move onto the toolbar from the chassis: the shared grace keeps it open on the way.
  await controls.first().hover();
  for (let i = 0; i < count; i++) {
    const centre = await elementCentre(controls.nth(i));
    expect(await topmostChromeAt(page, centre)).toBe("fixture-compressor");
  }
  // The open toolbar did not raise its chassis.
  expect(await paintLayer(occupant)).toBe("front-object");
});

test("the workbench toolbar lives in the interaction layer without raising the DUT", async ({
  page,
}) => {
  await page.goto("/devices/synth_voice");
  await expect(page.locator('[data-geometry="current"]').first()).toBeAttached({ timeout: 15000 });

  const dut = stageObject(page, "dev");
  const before = await renderedLayer(dut);
  expect(await paintLayer(dut)).toBe("rear-object"); // both faces shown: below the cable pass
  await page.locator('[data-face-instance="workbench:dut-front:dev"]').hover();
  const toolbar = page.locator('[data-chrome-for="dev"]');
  await expect(toolbar).toHaveClass(/open/);

  const open = toolbar.getByRole("button", { name: /^open / });
  const openBox = await open.boundingBox();
  const dutBox = await dut.boundingBox();
  if (!openBox || !dutBox) throw new Error("bench toolbar or DUT did not render");
  // Anchored flush above the device group's top edge, at its home + offset.
  expect(openBox.y + openBox.height).toBeLessThanOrEqual(dutBox.y + 1);
  expect(Math.abs(openBox.x - dutBox.x)).toBeLessThan(20);
  await open.hover();
  expect(await topmostChromeAt(page, await elementCentre(open))).toBe("dev");
  expect(await renderedLayer(dut)).toBe(before);
});
