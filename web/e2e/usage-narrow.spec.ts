import { expect, test, type Locator, type Page } from "@playwright/test";

/**
 * #1015: the Usage page's narrow frames, the Limits spacing and the spinner while a read is
 * held were only ever seen in jsdom. These cases open the mock app (web/mock) in Playwright's
 * headless shell at phone and design-narrow widths, then measure the boxes the layout draws:
 * no sideways page scroll, the frame gaps the CSS declares, and the loading states while the
 * reads are held.
 *
 * The sparse frame is the mock's own one-provider report. The dense frame serves a full
 * 30-day, four-provider report from the test over the same route, so neither the mock data
 * nor product code changes. The mock plugin has no per-route hold, so the reads are gated
 * with page.route, the same pattern as e2e/library-first-load.spec.ts.
 */

const USAGE = "#/usage";

async function openUsage(page: Page, scenario = "default") {
  await page.goto("/__mock/scenario/" + scenario, { waitUntil: "domcontentloaded" });
  await page.goto("/" + USAGE, { waitUntil: "domcontentloaded" });
  await expect(page.getByRole("heading", { name: "Usage", exact: true })).toBeVisible();
}

const DAY_MS = 86_400_000;
const day = (back: number) => new Date(Date.now() - back * DAY_MS).toISOString().slice(0, 10);

/** A full report for the same GET /api/usage the mock answers sparely: four providers, six models, thirty days. */
function denseReport() {
  const priced = (tokens: number) => Number(((tokens / 1_000_000) * 3).toFixed(2));
  const days = (seed: number, price: boolean) =>
    Array.from({ length: 30 }, (_, at) => ({
      date: day(29 - at),
      tokens: seed + at * 1_337,
      estimated_cost: price ? priced(seed + at * 1_337) : null,
    }));
  const model = (name: string, tokens: number, price: boolean) => ({
    model: name,
    input_tokens: Math.round(tokens * 0.2),
    output_tokens: Math.round(tokens * 0.1),
    cache_read_tokens: tokens - Math.round(tokens * 0.2) - Math.round(tokens * 0.1),
    cache_write_tokens: 0,
    estimated_cost: price ? priced(tokens) : null,
    cost_split: price ? { input: 0.4, output: 0.9, cache_read: 0.7, cache_write: 0 } : null,
    tiers: [{ tier: "standard", tokens, estimated_cost: price ? priced(tokens) : null }],
    unpriced_tokens: price ? 0 : tokens,
    days: days(tokens / 30, price),
  });
  return {
    host_id: "mock-dense",
    scanned_at: new Date().toISOString(),
    engines: [
      {
        engine: "claude",
        status: "scanned",
        files_scanned: 42,
        files_skipped: 2,
        models: [
          model("claude-opus-4-5-20251101-with-a-long-name-to-push-the-model-table", 12_000_000, true),
          model("claude-sonnet-3-7", 8_000_000, true),
          model("claude-haiku-4-5", 2_000_000, true),
        ],
        days: days(500_000, true),
      },
      {
        engine: "codex",
        status: "scanned",
        files_scanned: 11,
        files_skipped: 0,
        models: [model("gpt-6-luna", 6_000_000, true), model("gpt-5-codex", 3_000_000, true)],
        days: days(300_000, true),
      },
      {
        engine: "cursor-agent",
        status: "scanned",
        files_scanned: 7,
        files_skipped: 1,
        models: [model("composer-2", 5_000_000, false)],
        days: days(150_000, false),
      },
      { engine: "gemini-acp", status: "no_history", files_scanned: 0, files_skipped: 0, models: [], days: [] },
    ],
  };
}

interface FrameBox {
  kind: string;
  top: number;
  bottom: number;
  left: number;
  right: number;
  width: number;
  height: number;
}

/** The usage body's frames and scroll boxes, as the browser lays them out. */
async function usageFrames(page: Page) {
  return page.evaluate(() => {
    const body = document.querySelector(".usage-body");
    if (!(body instanceof HTMLElement)) throw new Error("no .usage-body");
    const rect = (node: Element) => {
      const box = node.getBoundingClientRect();
      return { top: box.top, bottom: box.bottom, left: box.left, right: box.right, width: box.width, height: box.height };
    };
    const scroller = document.querySelector(".usage-scroll");
    const main = document.querySelector("#main");
    if (!(scroller instanceof HTMLElement) || !(main instanceof HTMLElement)) throw new Error("no page scroller");
    return {
      body: { ...rect(body), paddingLeft: parseFloat(getComputedStyle(body).paddingLeft), paddingRight: parseFloat(getComputedStyle(body).paddingRight) },
      children: [...body.children].map((child) => ({ kind: (child as HTMLElement).className.trim().split(/\s+/)[0] ?? "", ...rect(child) })),
      main: { scrollWidth: main.scrollWidth, clientWidth: main.clientWidth },
      scroller: { scrollWidth: scroller.scrollWidth, clientWidth: scroller.clientWidth },
    };
  });
}

/** No horizontal scrollbar on the page or its scroller: only a frame inside may scroll sideways. */
function expectNoSidewaysScroll(frames: { main: { scrollWidth: number; clientWidth: number }; scroller: { scrollWidth: number; clientWidth: number } }) {
  expect(frames.main.scrollWidth - frames.main.clientWidth, "#main scrolls sideways").toBeLessThanOrEqual(1);
  expect(frames.scroller.scrollWidth - frames.scroller.clientWidth, ".usage-scroll scrolls sideways").toBeLessThanOrEqual(1);
}

/** Every top frame stays inside the body's content box, 24px apart (12px before the detail line). */
function expectFrameLayout(frames: { body: { left: number; right: number; paddingLeft: number; paddingRight: number }; children: FrameBox[] }, padding: number) {
  expect(frames.children.length, "usage frames").toBeGreaterThanOrEqual(3);
  expect(Math.abs(frames.body.paddingLeft - padding), "body left padding").toBeLessThanOrEqual(0.5);
  expect(Math.abs(frames.body.paddingRight - padding), "body right padding").toBeLessThanOrEqual(0.5);
  const left = frames.body.left + frames.body.paddingLeft;
  const right = frames.body.right - frames.body.paddingRight;
  for (const child of frames.children) {
    expect(child.left, child.kind + " left edge").toBeGreaterThanOrEqual(left - 0.5);
    expect(child.right, child.kind + " right edge").toBeLessThanOrEqual(right + 0.5);
  }
  for (let at = 1; at < frames.children.length; at += 1) {
    const previous = frames.children[at - 1]!;
    const child = frames.children[at]!;
    const gap = child.top - previous.bottom;
    const declared = child.kind === "usage-note" ? 12 : 24;
    expect(Math.abs(gap - declared), "gap before " + child.kind).toBeLessThanOrEqual(1);
  }
}

/** The three figures share one row, one width and the declared 12px gap. */
async function expectTiles(page: Page) {
  const tiles = await page.evaluate(() => {
    const grid = document.querySelector(".usage-tiles");
    if (!(grid instanceof HTMLElement)) throw new Error("no .usage-tiles");
    return {
      gap: parseFloat(getComputedStyle(grid).columnGap),
      boxes: [...grid.children].map((child) => {
        const box = child.getBoundingClientRect();
        return { top: box.top, left: box.left, right: box.right, width: box.width };
      }),
    };
  });
  expect(tiles.boxes).toHaveLength(3);
  expect(tiles.gap).toBe(12);
  expect(new Set(tiles.boxes.map((box) => Math.round(box.top))).size, "one tile row").toBe(1);
  for (let at = 1; at < tiles.boxes.length; at += 1) {
    expect(Math.abs(tiles.boxes[at]!.left - tiles.boxes[at - 1]!.right - 12), "tile gap").toBeLessThanOrEqual(1);
    expect(Math.abs(tiles.boxes[at]!.width - tiles.boxes[0]!.width), "tile widths").toBeLessThanOrEqual(1);
  }
}

/** The wide model table scrolls inside .usage-table and never widens the page. */
async function expectTableScroller(page: Page) {
  const table = await page.evaluate(() => {
    const scroller = document.querySelector(".usage-table");
    if (!(scroller instanceof HTMLElement)) throw new Error("no .usage-table");
    return { clientWidth: scroller.clientWidth, scrollWidth: scroller.scrollWidth, overflowX: getComputedStyle(scroller).overflowX };
  });
  expect(["auto", "scroll"]).toContain(table.overflowX);
  expect(table.scrollWidth, "model table is wider than the phone").toBeGreaterThan(table.clientWidth);
  const reachable = await page.evaluate(() => {
    const scroller = document.querySelector(".usage-table");
    if (!(scroller instanceof HTMLElement)) throw new Error("no .usage-table");
    scroller.scrollLeft = scroller.scrollWidth;
    const box = scroller.getBoundingClientRect();
    const last = scroller.querySelector("th:last-child");
    if (!(last instanceof HTMLElement)) throw new Error("no last column");
    return { lastRight: last.getBoundingClientRect().right, boxRight: box.right };
  });
  expect(reachable.lastRight, "last column stays inside the table's own scroll").toBeLessThanOrEqual(reachable.boxRight + 1);
}

async function expectSpinner(locator: Locator, what: string) {
  await expect(locator, what).toBeVisible();
  const animation = await locator.evaluate((element) => {
    const style = getComputedStyle(element);
    return { name: style.animationName, duration: style.animationDuration };
  });
  expect(animation.name, what + " animation").toBe("spin");
  expect(animation.duration, what + " runs").not.toBe("0s");
}

/** Reads held at the page; a strict-mode double mount can hold two at once. */
async function routeHeld(page: Page, pattern: string) {
  const gates: (() => void)[] = [];
  await page.route(pattern, async (route) => {
    await new Promise<void>((resolve) => gates.push(resolve));
    try {
      if (!route.request().failure()) await route.continue();
    } catch {
      // The page aborted this read (a strict-mode replay), so there is nothing to continue.
    }
  });
  return {
    async waiting() {
      await expect.poll(() => gates.length, { message: "a read is held" }).toBeGreaterThan(0);
    },
    /** Releases every read waiting now. */
    release() {
      for (const gate of gates.splice(0)) gate();
    },
  };
}

for (const width of [320, 390]) {
  test("the sparse usage frame fits the " + width + "px phone viewport", async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await openUsage(page);
    await expect(page.getByRole("row", { name: /claude-sonnet/ })).toBeVisible();
    await expect(page.getByRole("region", { name: "Totals" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Providers" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Breakdown" })).toBeVisible();

    const frames = await usageFrames(page);
    expectNoSidewaysScroll(frames);
    expectFrameLayout(frames, 16);
    await expectTiles(page);
    await expectTableScroller(page);

    // The chart and the providers stack, not sit side by side, at a phone width.
    const stacked = await page.evaluate(() => {
      const plot = document.querySelector(".usage-plot")?.getBoundingClientRect();
      const summary = document.querySelector(".usage-summary")?.getBoundingClientRect();
      const chart = document.querySelector(".usage-chart svg")?.getBoundingClientRect();
      return { plotBottom: plot?.bottom ?? 0, summaryTop: summary?.top ?? 0, chartWidth: chart?.width ?? 0 };
    });
    expect(stacked.summaryTop, "providers sit under the chart").toBeGreaterThanOrEqual(stacked.plotBottom - 1);
    expect(stacked.chartWidth, "chart fits the phone").toBeGreaterThan(0);

    // The cost frame adds its split and tier figures and still fits.
    await page.getByRole("radio", { name: "Cost" }).click();
    await expect(page.getByRole("heading", { name: "Cost by token type" })).toBeVisible();
    await expect(page.getByRole("heading", { name: "Cost by tier" })).toBeVisible();
    const cost = await usageFrames(page);
    expectNoSidewaysScroll(cost);
    expectFrameLayout(cost, 16);
  });
}

for (const width of [320, 390]) {
  test("the dense usage frame keeps its wide table inside its own scroller at " + width + "px", async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    await page.route("**/api/usage?**", (route) => route.fulfill({ json: denseReport() }));
    await openUsage(page);
    await expect(page.locator(".usage-table tbody tr")).toHaveCount(6);
    await expect(page.getByRole("button", { name: /^Set price for claude-opus-4-5/ })).toBeVisible();
    await expect(page.getByLabel("Scan status")).toContainText("Gemini CLI: no local history");
    const share = page.getByRole("group", { name: "Share of tokens" });
    await expect(share).toBeVisible();
    expect(await share.locator(":scope > span").count()).toBe(3);

    const frames = await usageFrames(page);
    expectNoSidewaysScroll(frames);
    expectFrameLayout(frames, 16);
    await expectTiles(page);
    await expectTableScroller(page);

    // The cost frame is denser still: split, tiers, the same table. It fits too.
    await page.getByRole("radio", { name: "Cost" }).click();
    await expect(page.getByRole("heading", { name: "Cost by token type" })).toBeVisible();
    const cost = await usageFrames(page);
    expectNoSidewaysScroll(cost);
    expectFrameLayout(cost, 16);
    expect(cost.children.length, "cost frame adds a section").toBeGreaterThan(frames.children.length);
  });
}

test("the first usage read shows its loading state and heading spinner while it is held", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 800 });
  const held = await routeHeld(page, "**/api/usage?**");
  try {
    await page.goto("/__mock/scenario/default", { waitUntil: "domcontentloaded" });
    await page.goto("/" + USAGE, { waitUntil: "domcontentloaded" });
    await held.waiting();

    const refresh = page.getByRole("button", { name: "Refresh usage" });
    await expect(refresh).toBeDisabled();
    await expectSpinner(refresh.locator("svg.spin"), "heading refresh spinner");
    // First visit: after the 400ms wait the page says what the one slow scan is doing.
    await expect(page.getByText("Reading your history…")).toBeVisible();
    await expect(page.getByText("This happens once.")).toBeVisible();
    await expect(page.getByRole("table")).toHaveCount(0);
    const loading = await usageFrames(page);
    expectNoSidewaysScroll(loading);
  } finally {
    held.release();
  }
  await expect(page.getByRole("row", { name: /claude-sonnet/ })).toBeVisible();
  await expect(page.getByRole("button", { name: "Refresh usage" })).toBeEnabled();
  await expect(page.getByRole("button", { name: "Refresh usage" }).locator("svg.spin")).toHaveCount(0);
  await expect(page.getByText("Reading your history…")).toHaveCount(0);
  const loaded = await usageFrames(page);
  expectNoSidewaysScroll(loaded);
  expectFrameLayout(loaded, 16);
});

for (const width of [640, 390, 320]) {
  test("the Limits frame sits 16px under its tabs and spins while its check is held at " + width + "px", async ({ page }) => {
    await page.setViewportSize({ width, height: 800 });
    const held = await routeHeld(page, "**/api/limits*");
    await openUsage(page, "limits");
    await expect(page.getByRole("row", { name: /claude-sonnet/ })).toBeVisible();
    await page.getByRole("tab", { name: "Limits" }).click();

    // The check is held: Claude Code's row stays and Codex shows its placeholder spinner.
    // The narrow frame drops the group intro line that a wide window shows.
    await held.waiting();
    const asking = page.locator(".usage-body.limits");
    await expect(asking).toBeVisible();
    await expect(asking.locator(".limit-win[aria-hidden]")).toBeVisible();
    await expect(asking.getByText("Weekly limit")).toBeVisible();
    await expect(page.getByRole("meter")).toHaveCount(0);
    await expectSpinner(asking.locator(".limit-win[aria-hidden] .spin"), "asking spinner");
    expect(await asking.locator(".limit-intro").evaluate((element) => getComputedStyle(element).display), "intro hidden in the narrow frame").toBe("none");
    await expect(page.getByText("Not reported")).toBeVisible();
    const checking = await usageFrames(page);
    expectNoSidewaysScroll(checking);

    try {
      held.release();
      await expect(page.getByRole("heading", { name: /^Codex/ })).toBeVisible();
      await expect(page.getByText("Weekly limit")).toBeVisible();
      await expect(page.getByText("62% used")).toBeVisible();
      await expect(page.locator(".limits-age")).toHaveText(/^Checked (just now|\d+ min ago)$/);
      const refresh = page.getByRole("button", { name: "Refresh limits" });
      await expect(refresh).toBeEnabled();
      await expect(page.locator(".usage-body.limits .spin")).toHaveCount(0);

      // The frames: the group label is first, 16px under the tab divider, and later groups keep that space.
      const spacing = await page.evaluate(() => {
        const tabs = document.querySelector(".usage-tabs");
        if (!(tabs instanceof HTMLElement)) throw new Error("no .usage-tabs");
        const tabsBottom = tabs.getBoundingClientRect().bottom;
        const headings = [...document.querySelectorAll(".limit-group")].map((group) => {
          const heading = group.querySelector("h2");
          const last = group.lastElementChild;
          if (!(heading instanceof HTMLElement)) throw new Error("group without a heading");
          const range = document.createRange();
          range.selectNodeContents(heading);
          return {
            first: group.firstElementChild === heading,
            text: heading.textContent ?? "",
            textTop: range.getBoundingClientRect().top,
            bottom: last === null ? 0 : last.getBoundingClientRect().bottom,
          };
        });
        return { tabsBottom, headings };
      });
      expect(spacing.headings.length, "one group per reporting engine").toBe(2);
      expect(spacing.headings.map((heading) => heading.text).join(" ")).toContain("Codex");
      expect(spacing.headings[0]!.first, "the group label is first").toBe(true);
      expect(Math.abs(spacing.headings[0]!.textTop - spacing.tabsBottom - 16), "Limits 16px under the tabs").toBeLessThanOrEqual(1);
      for (let at = 1; at < spacing.headings.length; at += 1) {
        const gap = spacing.headings[at]!.textTop - spacing.headings[at - 1]!.bottom;
        expect(Math.abs(gap - 16), "later group label space").toBeLessThanOrEqual(1);
      }
      const loaded = await usageFrames(page);
      expectNoSidewaysScroll(loaded);

      // A person's refresh keeps the figures and spins while it runs.
      await refresh.click();
      await held.waiting();
      await expect(refresh).toBeDisabled();
      await expectSpinner(refresh.locator("svg.spin"), "limits refresh spinner");
      await expect(page.locator(".limits-age")).toHaveText("Checking…");
      await expect(page.getByText("62% used")).toBeVisible();
      const refreshing = await usageFrames(page);
      expectNoSidewaysScroll(refreshing);
      held.release();
      await expect(page.locator(".limits-age")).toHaveText(/^Checked (just now|\d+ min ago)$/);
      await expect(refresh).toBeEnabled();
      await expect(page.locator(".limits-age")).not.toHaveText("Checking…");
    } finally {
      held.release();
    }
  });
}
