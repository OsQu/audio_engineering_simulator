import { expect, test } from "@playwright/test";
import { elementCentre, jack, openFixture, stageObject } from "./support/fixture-page";

interface KnownRegression {
  id: string;
  owner: "10.7";
  title: string;
}

function expectKnownFailure(regression: KnownRegression): void {
  test.fail(true, `Story ${regression.owner}: ${regression.id} — ${regression.title}`);
}

async function releaseWithoutFinalMove(
  page: Parameters<typeof openFixture>[0],
  point: { x: number; y: number },
): Promise<void> {
  await page.evaluate(
    ({ x, y }) =>
      window.dispatchEvent(
        new PointerEvent("pointerup", {
          bubbles: true,
          button: 0,
          buttons: 0,
          clientX: x,
          clientY: y,
          pointerId: 1,
          pointerType: "mouse",
        }),
      ),
    point,
  );
}

test("click-click and drag release at the current target produce the same connection", async ({
  page,
}) => {
  expectKnownFailure({
    id: "drag-release-target-differs-from-click-target",
    owner: "10.7",
    title: "release must resolve its current target, not the previous pointer-move verdict",
  });
  await openFixture(page, "patch-gestures");

  await jack(page, "gesture-source:output:0").click();
  await jack(page, "gesture-sink:input:0").click();
  await expect(page.locator("path.cable-core")).toHaveCount(1);

  await openFixture(page, "patch-gestures");
  const source = await elementCentre(jack(page, "gesture-source:output:0"));
  const target = await elementCentre(jack(page, "gesture-sink:input:0"));
  await page.mouse.move(source.x, source.y);
  await page.mouse.down();
  await page.mouse.move(source.x + 20, source.y);
  await releaseWithoutFinalMove(page, target);

  await expect(page.locator("path.cable-core")).toHaveCount(1);
});

test("near sockets do not become device drags at minimum and maximum zoom", async ({ page }) => {
  expectKnownFailure({
    id: "near-socket-press-starts-device-drag",
    owner: "10.7",
    title: "screen-space socket tolerance must block device moves at both zoom extremes",
  });
  for (const wheelDelta of [100_000, -100_000]) {
    await openFixture(page, "patch-gestures");
    const source = jack(page, "gesture-source:output:0");
    const object = stageObject(page, "gesture-source");
    const socket = await elementCentre(source);
    await page.mouse.move(socket.x, socket.y);
    await page.mouse.wheel(0, wheelDelta);

    const socketBox = await source.boundingBox();
    const objectBefore = await object.boundingBox();
    if (!socketBox || !objectBefore) throw new Error("fixture source did not render after zoom");
    const near = { x: socketBox.x + socketBox.width + 4, y: socketBox.y + socketBox.height / 2 };
    await page.mouse.move(near.x, near.y);
    await page.mouse.down();
    await page.mouse.move(near.x + 45, near.y + 20);
    await page.mouse.up();
    const objectAfter = await object.boundingBox();

    expect(objectAfter).toEqual(objectBefore);
  }
});
