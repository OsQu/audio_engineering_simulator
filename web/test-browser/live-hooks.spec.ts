// The live-verification hooks (`.claude/skills/verify-live/SKILL.md`): the stable `data-testid`s and their
// `data-*` values that agents read off the running simulator. This spec pins every one of them on the
// real studio + workbench with the real AudioWorklet, so renaming a hook (or breaking its value) fails
// here instead of silently breaking the skill. It runs the skill's own default-scene path end to end.
import { expect, type Locator, type Page, test } from "@playwright/test";

const DBFS = /^(-inf|-?\d+\.\d)$/;

/** Collect console errors + uncaught page errors for the whole test (the skill's "no console errors"). */
function collectErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on("console", (m) => {
    if (m.type() === "error") errors.push(m.text());
  });
  page.on("pageerror", (e) => errors.push(e.message));
  return errors;
}

async function expectHealthy(page: Page): Promise<void> {
  await expect(page.getByTestId("engine-status")).toHaveAttribute("data-state", "playing", {
    timeout: 30_000,
  });
  await expect(page.getByTestId("engine-status")).toHaveAttribute("data-sample-rate", "48000");
  const overruns = page.getByTestId("engine-overruns");
  await expect(overruns).toHaveAttribute("data-value", /^\d+$/, { timeout: 10_000 });
  await expect(overruns).toHaveAttribute("data-quanta", /^\d+$/);
  const drops = page.getByTestId("engine-drops");
  await expect(drops).toHaveAttribute("data-event", /^\d+$/);
  await expect(drops).toHaveAttribute("data-param", /^\d+$/);
}

const holdDbfs = async (peak: Locator): Promise<string> =>
  (await peak.getAttribute("data-hold-dbfs")) ?? "";

test("studio: ?debug=1, start, play a note — health, peak, meters and no console errors", async ({
  page,
}) => {
  const errors = collectErrors(page);
  await page.goto("/?debug=1");
  await expect(page.getByTestId("engine-status")).toHaveCount(0); // the Debug menu mounts once started
  await page.getByRole("button", { name: "▶ start" }).click();

  // `?debug=1` opened the Debug menu on load.
  await expect(page.getByTestId("debug-panel")).toHaveAttribute("open", "");
  await expectHealthy(page);

  // Silent until a note is played.
  const peak = page.getByTestId("output-peak");
  await expect(peak).toHaveAttribute("data-dbfs", DBFS);
  expect(await holdDbfs(peak)).toBe("-inf");

  // Play the default scene the way the skill does: open the MIDI controller's window (its keybed takes
  // QWERTY), then hold a key. The peak hold keeps the level after the release.
  await page.getByRole("button", { name: "ctrl — drag to move" }).hover();
  await page.getByRole("button", { name: "open MIDI Controller" }).click();
  await page.keyboard.down("a");
  await expect.poll(() => holdDbfs(peak), { timeout: 10_000 }).not.toBe("-inf");
  await page.keyboard.up("a");
  expect(Number(await holdDbfs(peak))).toBeGreaterThan(-60);

  // Device meters carry device/label/unit/value.
  const meter = page.getByTestId("meter").first();
  await expect(meter).toHaveAttribute("data-device", /.+/);
  await expect(meter).toHaveAttribute("data-label", /.+/);
  await expect(meter).toHaveAttribute("data-unit", /.+/);
  await expect(meter).toHaveAttribute("data-value", DBFS);

  // Overrun *counts* aren't asserted: under parallel Chromium workers the real worklet can starve. The
  // hook's shape is pinned above; the skill reports the number.
  expect(errors).toEqual([]);
});

test("workbench: the Debug drawer carries the same health and peak hooks", async ({ page }) => {
  const errors = collectErrors(page);
  await page.goto("/devices/synth_voice?debug=1");
  await expect(page.getByTestId("debug-panel")).toBeVisible({ timeout: 30_000 });
  // The bench boots suspended (autoplay policy) and resumes on the first interaction.
  await expect(page.getByTestId("engine-status")).toHaveAttribute("data-state", "suspended");
  await page.getByTestId("debug-panel").click();
  await expectHealthy(page);
  await expect(page.getByTestId("output-peak")).toHaveAttribute("data-dbfs", DBFS);
  await expect(page.getByTestId("output-peak")).toHaveAttribute("data-hold-dbfs", DBFS);
  expect(errors).toEqual([]);
});
