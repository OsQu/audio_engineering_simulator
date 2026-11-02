// Story 10.6: ordinary cables and portal stubs come from one endpoint resolution. Visible leads meet
// their sockets, hidden-face leads leave at the nearest chassis edge and are clipped from their owner,
// portals obey the same rules, and leads follow a drag preview before release.
import { expect, type Page, test } from "@playwright/test";
import {
  elementCentre,
  jack,
  objectChrome,
  openFixture,
  stageObject,
} from "./support/fixture-page";

type Pt = { x: number; y: number };

/** A cable hit path's start point (`getPointAtLength(0)`) in client px. */
const cableStart = (page: Page, key: string): Promise<Pt> =>
  page.locator(`[data-cable="${key}"]`).evaluate((el) => {
    const path = el as SVGPathElement;
    const ctm = path.getScreenCTM();
    if (!ctm) throw new Error("no CTM");
    const p = new DOMPoint(path.getPointAtLength(0).x, path.getPointAtLength(0).y).matrixTransform(
      ctm,
    );
    return { x: p.x, y: p.y };
  });

/** The clip-path urls on the ancestors of a cable's hit path (its pass clip chain). */
const clipChain = (page: Page, key: string): Promise<string[]> =>
  page.locator(`[data-cable="${key}"]`).evaluate((el) => {
    const ids: string[] = [];
    for (let n = el.parentElement; n; n = n.parentElement) {
      const clip = n.getAttribute("clip-path");
      if (clip) ids.push(clip);
    }
    return ids;
  });

/** The point lies on one edge of the device's rendered chassis box (within 2 px), within its span. */
async function edgeOf(page: Page, deviceId: string, p: Pt): Promise<string> {
  const box = await stageObject(page, deviceId).boundingBox();
  if (!box) throw new Error(`${deviceId} did not render`);
  const within = (v: number, lo: number, hi: number) => v > lo - 2 && v < hi + 2;
  const edges = {
    bottom: Math.abs(p.y - (box.y + box.height)) < 2 && within(p.x, box.x, box.x + box.width),
    right: Math.abs(p.x - (box.x + box.width)) < 2 && within(p.y, box.y, box.y + box.height),
    left: Math.abs(p.x - box.x) < 2 && within(p.y, box.y, box.y + box.height),
    top: Math.abs(p.y - box.y) < 2 && within(p.x, box.x, box.x + box.width),
  };
  return Object.entries(edges).find(([, on]) => on)?.[0] ?? "interior";
}

test("a visible rear-socket lead starts at its socket centre", async ({ page }) => {
  await openFixture(page, "cable-hover");
  const start = await cableStart(page, "rear-source:0->rear-sink:0");
  const socket = await elementCentre(jack(page, "rear-source:output:0"));
  expect(Math.hypot(start.x - socket.x, start.y - socket.y)).toBeLessThan(2);
  expect(await clipChain(page, "rear-source:0->rear-sink:0")).toEqual([]);
});

test("a hidden-face lead exits at its owner's nearest edge, clipped from the owner", async ({
  page,
}) => {
  await openFixture(page, "cable-hover");
  const key = "front-source:0->front-sink:0";
  // The generic Out jack sits near mid-height, so top vs bottom is a near tie decided by measured
  // face-local geometry (pure tie-break oracles live in cable-resolution.test.ts); what matters here is
  // that the lead starts on the chassis edge, never at an interior estimate.
  expect(await edgeOf(page, "front-source", await cableStart(page, key))).not.toBe("interior");
  const chain = await clipChain(page, key);
  expect(chain.some((c) => c.includes("main-out-front-source"))).toBe(true);
  expect(chain.some((c) => c.includes("main-out-front-sink"))).toBe(true);
  // Hidden-face ends need no foreground redraw (there is no visible socket to reach).
  await expect(page.locator('.cable-pass[data-pass="foreground"]')).toHaveCount(0);
});

test("portal stubs obey the ordinary endpoint rules", async ({ page }) => {
  await openFixture(page, "cable-portal");
  const rear = "rear-source:0->far-sink-a:0";
  const front = "front-source:0->far-sink-b:0";
  // Visible rear socket: the stub starts at the socket, unclipped.
  const start = await cableStart(page, rear);
  const socket = await elementCentre(jack(page, "rear-source:output:0"));
  expect(Math.hypot(start.x - socket.x, start.y - socket.y)).toBeLessThan(2);
  // Hidden face: the stub exits the chassis edge like a full cable, and is clipped from the owner.
  expect(await edgeOf(page, "front-source", await cableStart(page, front))).not.toBe("interior");
  expect((await clipChain(page, front)).some((c) => c.includes("main-out-front-source"))).toBe(
    true,
  );
  // Both chips render, above physical content.
  await expect(page.locator(`[data-portal="${rear}|from"]`)).toBeVisible();
  await expect(page.locator(`[data-portal="${front}|from"]`)).toBeVisible();
});

test("a lead follows its device's drag preview before release", async ({ page }) => {
  await openFixture(page, "cable-hover");
  const key = "rear-source:0->rear-sink:0";
  const box = await stageObject(page, "rear-source").boundingBox();
  if (!box) throw new Error("rear-source did not render");
  const before = await cableStart(page, key);

  await page.mouse.move(box.x + 4, box.y + 4);
  await page.mouse.down();
  await page.mouse.move(box.x + 4, box.y + 4 - 60, { steps: 4 });
  const during = await cableStart(page, key);
  const socket = await elementCentre(jack(page, "rear-source:output:0"));
  await page.mouse.up();

  expect(before.y - during.y).toBeGreaterThan(30);
  expect(Math.hypot(during.x - socket.x, during.y - socket.y)).toBeLessThan(2);
});

test("the workbench marks a hidden monitored socket with a labelled badge (U2)", async ({
  page,
}) => {
  await page.goto("/devices/da_converter");
  await expect(page.locator('[data-geometry="current"]').first()).toBeAttached({ timeout: 15000 });

  // Add a supporting Synth Voice and listen to its output.
  await page.locator("details.menu > summary").click();
  await page.getByRole("button", { name: "Synth Voice", exact: true }).click();
  const support = page.locator("[data-stage-object]");
  await expect(support).toHaveCount(2, { timeout: 15000 });
  const id = await support.evaluateAll(
    (els) =>
      els.map((e) => (e as HTMLElement).dataset.stageObject).find((v) => v && v !== "dev") ?? "",
  );
  const select = page.getByLabel("monitored output");
  const option = await select
    .locator("option", { hasText: "Synth Voice" })
    .evaluateAll((els) => (els as HTMLOptionElement[]).map((o) => o.value));
  await select.selectOption(option.find((v) => v.startsWith(`${id}:`)) ?? "");
  await expect(page.locator('[data-geometry="pending"]')).toHaveCount(0);

  const marker = page.locator(`[data-listen-marker="${id}"]`);
  const badge = page.locator(`[data-listen-badge="${id}"]`);
  await expect.poll(async () => (await marker.count()) + (await badge.count())).toBe(1);
  const hiddenFirst = (await badge.count()) === 1;

  // Rotating the device swaps a visible socket's ring for the hidden-socket badge, or back.
  // (The added device lands below the fold; this test is about the badge, not toolbar reach — click the
  // rotate control directly.)
  await objectChrome(page, id)
    .getByRole("button", { name: "rotate Synth Voice", includeHidden: true })
    .evaluate((b) => (b as HTMLButtonElement).click());
  await expect(hiddenFirst ? marker : badge).toHaveCount(1);
  await expect(hiddenFirst ? badge : marker).toHaveCount(0);
  // The badge is never a patch target.
  if (!hiddenFirst)
    expect(await badge.evaluate((el) => getComputedStyle(el).pointerEvents)).toBe("none");
});

/** The held lead's start point (client px), or null when no held lead is drawn. */
const heldStart = (page: Page): Promise<Pt | null> =>
  page.evaluate(() => {
    const path = document.querySelector<SVGPathElement>('.cable-pass[data-pass="held"] path');
    const ctm = path?.getScreenCTM();
    if (!path || !ctm) return null;
    const p0 = path.getPointAtLength(0);
    const p = new DOMPoint(p0.x, p0.y).matrixTransform(ctm);
    return { x: p.x, y: p.y };
  });

test("a held lead re-resolves its source across a view switch", async ({ page }) => {
  await openFixture(page, "patch-gestures");
  const source = jack(page, "gesture-source:output:0");
  await source.click();
  await expect(page.getByText(/Patching from/)).toBeVisible();
  const socket = await elementCentre(source);
  await expect.poll(async () => (await heldStart(page))?.x ?? Number.NaN).toBeCloseTo(socket.x, 0);

  // Another wall: the source is off-view, so only a floating end tracks the pointer.
  await page.getByRole("button", { name: "Back", exact: true }).click();
  await expect(page.locator('.cable-pass[data-pass="held"]')).toHaveCount(0);
  await page.mouse.move(400, 300);
  await expect(page.locator("circle.pending-end")).toHaveCount(1);

  // Back again: the lead starts at the socket's *current* projection, not a stale pick-time point.
  await page.getByRole("button", { name: "Front", exact: true }).click();
  await page.mouse.move(420, 320);
  const again = await elementCentre(source);
  await expect
    .poll(async () => {
      const at = await heldStart(page);
      return at ? Math.hypot(at.x - again.x, at.y - again.y) : Number.POSITIVE_INFINITY;
    })
    .toBeLessThan(2);
});

test("a held lead hides while its source flips, then exits the settled side's edge", async ({
  page,
}) => {
  await openFixture(page, "patch-gestures");
  await jack(page, "gesture-source:output:0").click();
  await expect.poll(() => heldStart(page)).not.toBeNull();

  await stageObject(page, "gesture-source").hover();
  await expect(objectChrome(page, "gesture-source")).toHaveClass(/open/);
  await objectChrome(page, "gesture-source")
    .getByRole("button", { name: "front", exact: true })
    .click();
  await expect(page.getByText(/Patching from/)).toBeVisible(); // the pick survives the flip

  // Settled front-on: the socket is on the hidden back, so the held lead exits the chassis edge.
  await expect(page.locator('[data-face-instance="studio:gesture-source"]')).toHaveAttribute(
    "data-face-state",
    "presented",
  );
  await page.mouse.move(600, 600);
  await expect
    .poll(async () => {
      const at = await heldStart(page);
      return at ? edgeOf(page, "gesture-source", at) : "none";
    })
    .not.toMatch(/none|interior/);
});
