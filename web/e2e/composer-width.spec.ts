import { expect, test, type Page } from "@playwright/test";
// @ts-expect-error plain module shared with the preview renderer
import { FRAMES } from "../tools/pr-preview/catalog.mjs";

type Layout = "console" | "ledger" | "reading";
type Width = "comfortable" | "wide" | "full";

async function openThread(page: Page, layout: Layout, chatWidth: Width) {
  await page.setViewportSize({ width: 2000, height: 1293 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addInitScript(({ layout, chatWidth }) => {
    if (sessionStorage.getItem("composer-width-seeded")) return;
    localStorage.setItem("orbyn.prefs.v1", JSON.stringify({
      workspaceLayout: layout, chatWidth, panelOpen: true, panelAnimationMs: 0,
      layouts: { "t-done": { panelTab: "diff", panelTabOrder: ["diff", "browser", "activity", "side"], panelTabsClosed: [], detailsOpen: true, terminalOpen: false, panelMaximized: false, panelWidth: 380, sideChat: null } },
    }));
    sessionStorage.setItem("composer-width-seeded", "1");
  }, { layout, chatWidth });
  await page.goto("/__mock/scenario/default?to=/%23/threads/t-done");
  await expect(page.locator(".transcript-inner")).toBeVisible();
  await expect(page.getByRole("tab", { name: "Changes", exact: true })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("separator", { name: "Resize panel" })).toBeVisible();
}

/** The text and composer share the available column, with the details rail outside it. */
async function expectColumn(page: Page, railShown: boolean) {
  const result = await page.evaluate((hasRail) => {
    const main = document.querySelector<HTMLElement>(".thread-main")!;
    const transcript = document.querySelector<HTMLElement>(".transcript-inner")!;
    const composer = document.querySelector<HTMLElement>(".composer-box")!;
    const rail = document.querySelector<HTMLElement>(".thread-main > .details");
    const mainBox = main.getBoundingClientRect();
    const t = transcript.getBoundingClientRect();
    const ts = getComputedStyle(transcript);
    const c = composer.getBoundingClientRect();
    const railBox = hasRail ? rail?.getBoundingClientRect() : undefined;
    const left = mainBox.left + 16;
    const right = (railBox?.left ?? mainBox.right) - 16;
    const available = right - left;
    const reading = getComputedStyle(main).getPropertyValue("--reading-w").trim();
    const maxWidth = reading.endsWith("%") ? available : parseFloat(reading);
    const width = Math.min(available, maxWidth);
    const expectedLeft = left + (available - width) / 2;
    return {
      railVisible: !!rail && getComputedStyle(rail).display !== "none",
      left: c.left, right: c.right, width: c.width,
      transcriptLeft: t.left + parseFloat(ts.paddingLeft),
      transcriptRight: t.right - parseFloat(ts.paddingRight),
      expectedLeft, expectedRight: expectedLeft + width, expectedWidth: width,
      railLeft: railBox?.left,
    };
  }, railShown);
  expect(result.railVisible).toBe(railShown);
  expect(result.left).toBeCloseTo(result.expectedLeft, 0);
  expect(result.right).toBeCloseTo(result.expectedRight, 0);
  expect(result.width).toBeCloseTo(result.expectedWidth, 0);
  expect(result.transcriptLeft).toBeCloseTo(result.expectedLeft, 0);
  expect(result.transcriptRight).toBeCloseTo(result.expectedRight, 0);
  if (result.railLeft !== undefined) expect(result.right).toBeLessThan(result.railLeft);
}

async function dragToWidth(page: Page, target: number) {
  const handle = page.getByRole("separator", { name: "Resize panel" });
  const pane = page.locator(".context");
  const handleBox = await handle.boundingBox();
  const paneBox = await pane.boundingBox();
  expect(handleBox).not.toBeNull();
  expect(paneBox).not.toBeNull();
  const y = handleBox!.y + 120;
  await page.mouse.move(handleBox!.x + handleBox!.width / 2, y);
  await page.mouse.down();
  await page.mouse.move(paneBox!.x + paneBox!.width - target, y, { steps: 8 });
  await expectColumn(page, true);
  await page.mouse.up();
  await expectColumn(page, true);
  await expect.poll(async () => Math.round((await pane.boundingBox())!.width)).toBe(target);
}

for (const width of ["wide", "full"] as const) {
  test(`Ledger ${width} keeps its column clear of the rail while resizing`, async ({ page }) => {
    await openThread(page, "ledger", width);
    await expectColumn(page, true);
    await dragToWidth(page, 720);
    await page.getByRole("tab", { name: "Changes", exact: true }).click();
    await expectColumn(page, true);
    await page.reload();
    await expect.poll(async () => Math.round((await page.locator(".context").boundingBox())!.width)).toBe(720);
    await expectColumn(page, true);
    await dragToWidth(page, 320);
    await page.reload();
    await expect.poll(async () => Math.round((await page.locator(".context").boundingBox())!.width)).toBe(320);
    await expectColumn(page, true);
    await page.getByRole("button", { name: "Hide sidebar" }).click();
    await expectColumn(page, true);
    await page.getByRole("button", { name: "Details", exact: true }).click();
    await expectColumn(page, false);
    await page.getByRole("button", { name: "Show sidebar" }).click();
    await expectColumn(page, false);
  });
}

for (const layout of ["console", "ledger", "reading"] as const) {
  test(`${layout} comfortable width uses its available column`, async ({ page }) => {
    await openThread(page, layout, "comfortable");
    await expectColumn(page, layout === "ledger");
  });
}

test("the shell-ledger preview opens the full-width Ledger rail", async ({ page }) => {
  const frame = FRAMES["shell-ledger"] as {
    route: string;
    steps: ({ do: "click"; role: "link" | "radio"; name: string; exact?: boolean; prefix?: boolean } | { do: "press"; key: string })[];
  };
  await page.setViewportSize({ width: 1400, height: 760 });
  await page.goto(`/__mock/scenario/default?to=${encodeURIComponent(`/${frame.route}`)}`);
  for (const step of frame.steps) {
    if (step.do === "press") await page.keyboard.press(step.key);
    else await page.getByRole(step.role, { name: step.prefix ? new RegExp(`^${step.name}`) : step.name, exact: step.exact }).first().click();
  }
  await expect(page.getByRole("region", { name: "Settings" })).toHaveCount(0);
  await expect(page.locator("html")).toHaveAttribute("data-workspace", "ledger");
  await expect(page.locator(".thread-main")).toHaveAttribute("data-chat-width", "full");
  await expect(page.locator(".thread-main > .details")).toBeVisible();
  await expect(page.locator(".context")).toHaveCount(0);
  await expectColumn(page, true);
});
