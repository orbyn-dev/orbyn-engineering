import { expect, test, type Page } from "@playwright/test";
import { BACKTEST_FRAMES } from "../src/features/crew/backtest/names";
import { CAPACITY_FRAMES } from "../src/features/crew/capacity/names";
import { ESCAPE_FRAMES } from "../src/features/crew/escape/names";
import { ESCAPE_LIVE_FRAMES } from "../src/features/crew/escape/live/screens";
import { ORACLE_FRAMES } from "../src/features/crew/oracle/names";
import { SETTINGS_FRAMES } from "../src/features/crew/settings/names";
// @ts-expect-error plain module shared with the preview renderer
import { FRAMES } from "../tools/pr-preview/catalog.mjs";

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

/** What each live escape frame must show once it has read the mock: text only a read can fill, never sample copy. */
const LIVE_PROOF: Record<(typeof ESCAPE_LIVE_FRAMES)[number], (page: Page) => ReturnType<Page["getByText"]>> = {
  "escape-log": (page) => page.getByText(/^As of /),
  "escape-narrow-window": (page) => page.getByText(/^Showing \d+ of \d+ · newest found first$/),
  "escape-narrow-panel": (page) => page.getByText(/are behind Open in Crew\./),
  "escape-empty": (page) => page.getByText("No escapes in the last 60 days"),
  "escape-low-sample": (page) => page.getByText(/Fewer than 10 merges in the last 60 days \(6\)/),
  "escape-error": (page) => page.getByText(/Can’t read escapes/),
  "escape-possible": (page) => page.getByText(/possible escapes?\./),
  "escape-detail": (page) => page.getByText("Timeline"),
  "escape-detail-tooling": (page) => page.getByText("Timeline"),
  "escape-confirmed": (page) => page.getByText("Timeline"),
  "escape-dismissed": (page) => page.getByText("Timeline"),
  "escape-gap": (page) => page.getByText(/What kind of gap was it\?/),
  "escape-check-proposed": (page) => page.getByText(/It must fail on the escaped merge and pass on/),
  "escape-check-writing": (page) => page.getByText(/writing the test file/),
  "escape-check-approve": (page) => page.getByText("Approve the proof"),
  "escape-check-proven": (page) => page.getByText("Queued like any change, with its own risk tier, holds and required checks"),
  "escape-check-invalid": (page) => page.getByText("The check did not prove itself"),
  "escape-check-retry": (page) => page.getByText("Retry this check?"),
  "escape-check-withdraw": (page) => page.getByText("Withdraw this check?"),
  "escape-check-withdrawn": (page) => page.getByText(/A new proposal replaces this one/),
  "escape-limit": (page) => page.getByText(/escape limit\. A new escape is not recorded/),
  "escape-settings": (page) => page.getByText("60 days · default"),
};

for (const frame of ESCAPE_LIVE_FRAMES) {
  test(`${frame} runs on the dev mock: it reads /api/crew/escapes, sends no write and shows live content`, async ({ page }) => {
    const entry = (FRAMES as Record<string, { route: string; scenario?: string; steps?: { do: string; role?: string; name?: string }[] }>)[frame]!;
    expect(entry, `${frame} is in the pr-preview catalog`).toBeTruthy();
    const reads: string[] = [];
    const writes: string[] = [];
    page.on("request", (request) => {
      if (!request.url().includes("/api/crew/escapes")) return;
      (request.method() === "GET" ? reads : writes).push(`${request.method()} ${request.url()}`);
    });
    await page.goto(`/__mock/scenario/${entry.scenario ?? "default"}`);
    await page.goto(`/${entry.route}`);
    const before = entry.steps?.length ? page.getByText("Timeline") : LIVE_PROOF[frame](page);
    await expect(before.first()).toBeVisible();
    for (const step of entry.steps ?? []) {
      if (step.do === "click" && step.role === "button" && step.name) await page.getByRole("button", { name: step.name }).click();
    }
    await expect(LIVE_PROOF[frame](page).first()).toBeVisible();
    await expect(page.getByText("Sample data.")).toHaveCount(0);
    expect(reads.length, "at least one GET to /api/crew/escapes").toBeGreaterThan(0);
    expect(writes).toEqual([]);
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
