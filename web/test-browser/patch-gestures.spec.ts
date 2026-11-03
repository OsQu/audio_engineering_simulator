// Story 10.7: the patch-target & gesture-lifecycle browser matrix — illegal-target feedback, drag/click
// agreement, cancellation (pointercancel, blur), multi-pointer, pending picks through a pan, and the
// workbench running the same arbiter. The zoom-extreme near-socket case and release-without-final-move
// live in known-regressions.spec.ts (graduated by this Story).
import { expect, type Page, test } from "@playwright/test";
import { elementCentre, jack, openFixture, stageObject } from "./support/fixture-page";

const SOURCE = "matrix-source:output:0";
const OTHER = "matrix-other:output:0";
const SINK = "matrix-sink:input:0";

const cables = (page: Page) => page.locator("path.cable-core");
const heldLead = (page: Page) => page.locator("path.cable-drag");

async function dispatchPointer(
  page: Page,
  type: string,
  at: { x: number; y: number },
  init: { pointerId?: number; isPrimary?: boolean; target?: "window" | "element" } = {},
): Promise<void> {
  await page.evaluate(
    ({ type, x, y, pointerId, isPrimary, target }) => {
      const event = new PointerEvent(type, {
        bubbles: true,
        cancelable: true,
        button: 0,
        buttons: type === "pointerup" || type === "pointercancel" ? 0 : 1,
        clientX: x,
        clientY: y,
        pointerId,
        isPrimary,
        pointerType: "mouse",
      });
      const el = target === "element" ? document.elementFromPoint(x, y) : null;
      (el ?? window).dispatchEvent(event);
    },
    {
      type,
      x: at.x,
      y: at.y,
      pointerId: init.pointerId ?? 1,
      isPrimary: init.isPrimary ?? true,
      target: init.target ?? "window",
    },
  );
}

/** A stage object's placement in world mm (its inline left/bottom), immune to camera refits. */
const placement = (page: Page, id: string) =>
  stageObject(page, id).evaluate(
    (el) => `${(el as HTMLElement).style.left}|${(el as HTMLElement).style.bottom}`,
  );

test("drag + drop onto a legal socket connects", async ({ page }) => {
  await openFixture(page, "patch-matrix");
  const from = await elementCentre(jack(page, SOURCE));
  const to = await elementCentre(jack(page, SINK));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 6 });
  await page.mouse.up();
  await expect(cables(page)).toHaveCount(1);
  await expect(heldLead(page)).toHaveCount(0);
});

test("a drag released on an illegal socket keeps the pick and shows why", async ({ page }) => {
  await openFixture(page, "patch-matrix");
  const from = await elementCentre(jack(page, SOURCE));
  const illegal = await elementCentre(jack(page, OTHER));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(illegal.x, illegal.y, { steps: 6 });
  await page.mouse.up();

  await expect(cables(page)).toHaveCount(0);
  await expect(page.locator("[data-patch-reason]")).toHaveText(/connect an output to an input/);
  await expect(page.getByText(/Patching from/)).toBeVisible();

  // The same pick then completes on a legal destination (click + click semantics).
  await jack(page, SINK).click();
  await expect(cables(page)).toHaveCount(1);
  await expect(page.getByText(/Patching from/)).toHaveCount(0);
});

test("a click on an illegal socket keeps the pick with the same reason as a drag", async ({
  page,
}) => {
  await openFixture(page, "patch-matrix");
  await jack(page, SOURCE).click();
  await jack(page, OTHER).click();
  await expect(cables(page)).toHaveCount(0);
  await expect(page.locator("[data-patch-reason]")).toHaveText(/connect an output to an input/);
});

test("pointercancel during a patch drag drops the lead and never connects", async ({ page }) => {
  await openFixture(page, "patch-matrix");
  const from = await elementCentre(jack(page, SOURCE));
  const to = await elementCentre(jack(page, SINK));
  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(to.x, to.y, { steps: 6 });
  await expect(heldLead(page)).toHaveCount(1);

  await dispatchPointer(page, "pointercancel", to);
  await expect(heldLead(page)).toHaveCount(0);
  await page.mouse.up();
  await expect(cables(page)).toHaveCount(0);
  await expect(page.getByText(/Patching from/)).toHaveCount(0);
});

test("window blur cancels a device move without committing it", async ({ page }) => {
  await openFixture(page, "patch-matrix");
  const before = await placement(page, "matrix-other");
  const box = await stageObject(page, "matrix-other").boundingBox();
  if (!box) throw new Error("fixture device did not render");
  // A chassis corner, far from the centred socket.
  const grab = { x: box.x + 6, y: box.y + box.height - 6 };
  await page.mouse.move(grab.x, grab.y);
  await page.mouse.down();
  await page.mouse.move(grab.x + 80, grab.y - 40, { steps: 5 });
  await expect(page.locator('[data-drag-outline="matrix-other"]')).toHaveCount(1);

  await page.evaluate(() => window.dispatchEvent(new Event("blur")));
  await expect(page.locator('[data-drag-outline="matrix-other"]')).toHaveCount(0);
  await page.mouse.up();
  expect(await placement(page, "matrix-other")).toBe(before);
});

test("a second pointer cannot start a competing gesture mid-patch", async ({ page }) => {
  await openFixture(page, "patch-matrix");
  const before = await placement(page, "matrix-other");
  const from = await elementCentre(jack(page, SOURCE));
  const to = await elementCentre(jack(page, SINK));
  const box = await stageObject(page, "matrix-other").boundingBox();
  if (!box) throw new Error("fixture device did not render");
  const body = { x: box.x + 6, y: box.y + box.height - 6 };

  await page.mouse.move(from.x, from.y);
  await page.mouse.down();
  await page.mouse.move(from.x + 30, from.y, { steps: 3 });
  // A second, non-primary pointer presses a device body and moves: ignored.
  await dispatchPointer(page, "pointerdown", body, {
    pointerId: 7,
    isPrimary: false,
    target: "element",
  });
  await dispatchPointer(page, "pointermove", { x: body.x + 90, y: body.y }, { pointerId: 7 });
  await dispatchPointer(page, "pointerup", { x: body.x + 90, y: body.y }, { pointerId: 7 });
  await expect(heldLead(page)).toHaveCount(1); // the patch drag is still held

  await page.mouse.move(to.x, to.y, { steps: 4 });
  await page.mouse.up();
  await expect(cables(page)).toHaveCount(1);
  expect(await placement(page, "matrix-other")).toBe(before);
});

test("a pending pick survives a backdrop pan and still completes", async ({ page }) => {
  await openFixture(page, "patch-matrix");
  await jack(page, SOURCE).click();
  await expect(page.getByText(/Patching from/)).toBeVisible();

  const viewport = page.locator(".viewport").first();
  const vb = await viewport.boundingBox();
  if (!vb) throw new Error("viewport did not render");
  // Empty floor well below the gear.
  const start = { x: vb.x + vb.width / 2, y: vb.y + vb.height - 20 };
  const sinkBefore = await elementCentre(jack(page, SINK));
  await page.mouse.move(start.x, start.y);
  await page.mouse.down();
  await page.mouse.move(start.x + 60, start.y - 10, { steps: 5 });
  await page.mouse.up();
  const sinkAfter = await elementCentre(jack(page, SINK));
  expect(Math.abs(sinkAfter.x - sinkBefore.x)).toBeGreaterThan(30); // the camera panned
  await expect(page.getByText(/Patching from/)).toBeVisible(); // the pick survived

  await jack(page, SINK).click();
  await expect(cables(page)).toHaveCount(1);
});

test("the workbench runs the same arbiter: a near-socket press starts a lead, not a move", async ({
  page,
}) => {
  await page.setViewportSize({ width: 1400, height: 1400 }); // the bench stack is tall
  await page.goto("/devices/synth_voice");
  await expect(page.locator('[data-face-instance][data-geometry="current"]').first()).toBeAttached({
    timeout: 15000,
  });
  // A DUT jack whose centre is on screen and actually under the pointer (the stage clips the rest).
  const box = await page.evaluate(() => {
    for (const el of document.querySelectorAll<HTMLElement>(
      '[data-face-instance^="workbench:dut-"] [data-jack]',
    )) {
      const r = el.getBoundingClientRect();
      const x = r.left + r.width / 2;
      const y = r.top + r.height / 2;
      if (r.width > 0 && x > 0 && y > 0 && x < innerWidth - 80 && y < innerHeight - 80) {
        if (el.contains(document.elementFromPoint(x, y))) {
          return { x: r.left, y: r.top, width: r.width, height: r.height };
        }
      }
    }
    return null;
  });
  if (!box) throw new Error("no on-screen bench socket");
  const group = page.locator('.device-group[data-stage-object="dev"]');
  const before = await group.evaluate((el) => (el as HTMLElement).style.transform);
  const near = { x: box.x + box.width + 3, y: box.y + box.height / 2 };

  await page.mouse.move(near.x, near.y);
  await page.mouse.down();
  await page.mouse.move(near.x + 60, near.y + 40, { steps: 5 });
  await expect(heldLead(page)).toHaveCount(1);
  await page.mouse.up();
  await page.keyboard.press("Escape");
  expect(await group.evaluate((el) => (el as HTMLElement).style.transform)).toBe(before);
});
