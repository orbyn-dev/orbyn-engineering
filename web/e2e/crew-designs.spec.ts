import { expect, test, type Page } from "@playwright/test";
import { BACKTEST_FRAMES } from "../src/features/crew/backtest/names";
import { CAPACITY_FRAMES } from "../src/features/crew/capacity/names";
import { ESCAPE_FRAMES } from "../src/features/crew/escape/names";
import { ESCAPE_LIVE_FRAMES } from "../src/features/crew/escape/live/screens";
import { ORACLE_FRAMES } from "../src/features/crew/oracle/names";
import { SETTINGS_FRAMES } from "../src/features/crew/settings/names";

/** The drawn Crew screens of Designs v20 to v23 in the dev build, where the Preview screens exist: sample data, no API request, no live control. */
const open = (page: Page, frame: string) => page.goto(`/#/preview/crew/${frame}`);
const REASON = "Crew is not connected to this app yet";

/** Escape screens that read the dev mock instead of drawing sample data; the loop below covers every other frame. */
const ESCAPE_DRAWN_FRAMES = ESCAPE_FRAMES.filter(
  (frame) => !(ESCAPE_LIVE_FRAMES as readonly string[]).includes(frame),
);

for (const frame of [...ESCAPE_DRAWN_FRAMES, ...CAPACITY_FRAMES, ...ORACLE_FRAMES, ...BACKTEST_FRAMES, ...SETTINGS_FRAMES]) {
  test(`${frame} draws on sample data, makes no Crew request and has no live control`, async ({ page }) => {
    const requests: string[] = [];
    page.on("request", (request) => {
      if (request.url().includes("/api/crew")) requests.push(request.url());
    });
    await open(page, frame);
    await expect(page.getByRole("note")).toContainText("Sample data.");
    const buttons = page.getByRole("button");
    const count = await buttons.count();
    for (let index = 0; index < count; index += 1) {
      await expect(buttons.nth(index)).toBeDisabled();
      await expect(buttons.nth(index)).toHaveAttribute("aria-description", REASON);
    }
    expect(requests).toEqual([]);
  });
}

for (const frame of ESCAPE_LIVE_FRAMES) {
  test(`${frame} runs on the dev mock instead of drawing sample data`, async ({ page }) => {
    await open(page, frame);
    await expect(page.getByText("Sample data.")).toHaveCount(0);
  });
}

test("a dialog frame shows its choices disabled, with the reason on the primary one", async ({ page }) => {
  await open(page, "capacity-paused-card");
  await expect(page.getByText("Starting it now is your decision and is recorded in the ledger.")).toBeVisible();
  const start = page.getByRole("button", { name: "Open the Review queue" });
  await expect(start).toBeDisabled();
  await expect(start).toHaveAttribute("title", REASON);
});

test("an unknown frame renders the normal app, not a Crew screen", async ({ page }) => {
  await open(page, "escape-nothing");
  await expect(page.getByText("Sample data.")).toHaveCount(0);
});
