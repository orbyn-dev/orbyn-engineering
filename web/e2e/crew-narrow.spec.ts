import { expect, test, type Page } from "@playwright/test";

/**
 * #1006: the 640 px sweep's three shared narrow rules were only ever seen by eye on Settings and the
 * thread header, because Crew could not be opened in the dev mock. These cases open the real
 * Settings › Crew pages at a 640 px window, where the Settings list leaves the page about 324 px,
 * and at 1280 px, then measure the boxes the rules draw:
 *
 * - the 840 px padding query drops a sheet page's nested padding from 28 px to 12 px;
 * - the tile grids (`.cx-t15map`, and `.cx-t10g` with its own narrow layout) keep the 150 px floor
 *   and one column on a narrow page, and their drawn columns again on a wide one;
 * - a header row (`.cx-t10hd`) and the report's two-column body (`.cx-t14two`) take their own rows
 *   instead of overflowing a narrow page, and one row again when there is width for it.
 *
 * Every page here uses the mock scenarios the Crew E2E cases already run, so it holds the same
 * sample reads as its Preview frame.
 */

const NARROW = 640;
const WIDE = 1280;
/** One 12.5 px header row with its 8 px box: anything taller has wrapped onto a second row. */
const ONE_ROW = 40;

/**
 * What each page draws at a wide window: the tile columns the drawing gives it, and whether its
 * header row fits on one line there. `0` columns leaves the count to the page (the dashboard's
 * tile row draws as many columns as it has figures). A page whose header already takes two rows at
 * 1280 px (the report's five items) says so.
 */
const PAGES = [
  { name: "the review coverage page", route: "#/settings/crew/capacity/coverage?repo=p-app", scenario: "crew-capacity", wideColumns: 3, wraps: true, oneRowWide: true },
  { name: "the escape log", route: "#/settings/crew/escapes?repo=p-app", scenario: "crew-escapes", wideColumns: 4, wraps: false, oneRowWide: true },
  { name: "the reviewer backtest report", route: "#/settings/crew/backtest/report?id=run-4n", scenario: "crew-backtest-report", wideColumns: 3, wraps: true, oneRowWide: false },
  { name: "the crew dashboard", route: "#/settings/crew/dashboard?repo=p-app", scenario: "crew", wideColumns: 0, wraps: true, oneRowWide: true },
] as const;

async function openCrew(page: Page, route: string, scenario: string) {
  await page.goto(`/__mock/scenario/${scenario}`, { waitUntil: "domcontentloaded" });
  await page.goto(`/${route}`, { waitUntil: "domcontentloaded" });
}

/** The page's own frame, and the boxes the three rules decide, as the browser painted them. */
async function boxes(page: Page) {
  return page.evaluate(() => {
    const rect = (node: Element) => {
      const box = node.getBoundingClientRect();
      return { top: box.top, bottom: box.bottom, left: box.left, right: box.right, width: box.width, height: box.height };
    };
    const grids = (selector: string) =>
      [...document.querySelectorAll(selector)].map((node) => ({
        box: rect(node),
        children: [...node.children].map((child) => rect(child)),
      }));
    const frame = document.querySelector(".cx-frame");
    if (!(frame instanceof HTMLElement)) throw new Error("no .cx-frame");
    const style = getComputedStyle(frame);
    const doc = document.documentElement;
    return {
      sideways: doc.scrollWidth - doc.clientWidth,
      sheet: frame.classList.contains("cx-sheetpage"),
      padding: { left: parseFloat(style.paddingLeft), right: parseFloat(style.paddingRight) },
      tiles: [...grids(".cx-t15map"), ...grids(".cx-t10g")],
      headers: grids(".cx-t10hd"),
      two: grids(".cx-t14two"),
    };
  });
}

/** The document itself never scrolls sideways: a narrow rule that failed would widen it. */
function expectNoSidewaysScroll(measured: { sideways: number }) {
  expect(measured.sideways, "the page scrolls sideways").toBeLessThanOrEqual(1);
}

/**
 * Every tile keeps the 150 px floor inside its grid, and the row holds the columns it draws:
 * one on a narrow page, and `columns` on a wide one (`0` leaves the count to the page).
 */
function expectTiles(tiles: { box: { right: number; width: number }; children: { left: number; right: number; width: number }[] }[], columns: number) {
  expect(tiles.length, "tile grids on the page").toBeGreaterThan(0);
  for (const tile of tiles) {
    expect(tile.children.length, "tiles in a grid").toBeGreaterThan(1);
    for (const child of tile.children) {
      expect(child.width, "a tile narrower than 150 px").toBeGreaterThanOrEqual(150);
      expect(child.right, "a tile past its grid").toBeLessThanOrEqual(tile.box.right + 1);
    }
    const lefts = new Set(tile.children.map((child) => Math.round(child.left))).size;
    if (columns === 0) continue;
    expect(lefts, "tile columns").toBe(Math.min(columns, tile.children.length));
  }
}

/** A wrapped row stays inside its row: an item that could not wrap would reach past the page. */
function expectHeaders(headers: { box: { right: number; width: number }; children: { right: number }[] }[]) {
  expect(headers.length, "header rows on the page").toBeGreaterThan(0);
  for (const header of headers) {
    for (const child of header.children) {
      expect(child.right, "a header item past its row").toBeLessThanOrEqual(header.box.right + 1);
    }
  }
}

for (const { name, route, scenario, wideColumns, wraps, oneRowWide } of PAGES) {
  test(`${name} keeps one tile column and a wrapped header at ${NARROW}px`, async ({ page }) => {
    await page.setViewportSize({ width: NARROW, height: 900 });
    await openCrew(page, route, scenario);
    await expect(page.locator(".cx-t15map, .cx-t10g").first()).toBeVisible();

    const measured = await boxes(page);
    expectNoSidewaysScroll(measured);
    expectTiles(measured.tiles, 1);
    expectHeaders(measured.headers);
    for (const header of measured.headers) {
      if (wraps) expect(header.box.height, "the header stays on one row").toBeGreaterThan(ONE_ROW);
      else expect(header.box.height, "the header wrapped with room to spare").toBeLessThanOrEqual(ONE_ROW);
    }
  });

  test(`${name} keeps its drawn columns and header row at ${WIDE}px`, async ({ page }) => {
    await page.setViewportSize({ width: WIDE, height: 900 });
    await openCrew(page, route, scenario);
    await expect(page.locator(".cx-t15map, .cx-t10g").first()).toBeVisible();

    const measured = await boxes(page);
    expectNoSidewaysScroll(measured);
    expectTiles(measured.tiles, wideColumns);
    expectHeaders(measured.headers);
    for (const header of measured.headers) {
      if (oneRowWide) expect(header.box.height, "the header wrapped with room to spare").toBeLessThanOrEqual(ONE_ROW);
    }
  });
}

test(`a Crew sheet page drops its nested padding at ${NARROW}px and keeps it at ${WIDE}px`, async ({ page }) => {
  await page.setViewportSize({ width: NARROW, height: 900 });
  await openCrew(page, "#/settings/crew/backtest", "crew-backtest");
  await expect(page.locator(".cx-frame.cx-sheetpage")).toBeVisible();
  const narrow = await boxes(page);
  expect(narrow.sheet, "the backtest page is a sheet page").toBe(true);
  expect(narrow.padding.left, "narrow sheet padding").toBe(12);
  expect(narrow.padding.right, "narrow sheet padding").toBe(12);

  await page.setViewportSize({ width: WIDE, height: 900 });
  await expect(page.locator(".cx-frame.cx-sheetpage")).toBeVisible();
  const wide = await boxes(page);
  expect(wide.padding.left, "wide sheet padding").toBe(28);
  expect(wide.padding.right, "wide sheet padding").toBe(28);
});

test(`the report's two columns stack at ${NARROW}px and sit side by side at ${WIDE}px`, async ({ page }) => {
  await page.setViewportSize({ width: NARROW, height: 900 });
  await openCrew(page, "#/settings/crew/backtest/report?id=run-4n", "crew-backtest-report");
  await expect(page.locator(".cx-t14two")).toBeVisible();
  const narrow = await boxes(page);
  expect(narrow.two.length, "two-column bodies").toBeGreaterThan(0);
  for (const body of narrow.two) {
    expect(body.children.length, "columns in a body").toBe(2);
    expect(new Set(body.children.map((child) => Math.round(child.left))).size, "one column at a narrow page").toBe(1);
    expect(body.children[1]!.top, "the second column sits under the first").toBeGreaterThanOrEqual(body.children[0]!.bottom - 1);
  }

  await page.setViewportSize({ width: WIDE, height: 900 });
  await expect(page.locator(".cx-t14two")).toBeVisible();
  const wide = await boxes(page);
  for (const body of wide.two) {
    expect(new Set(body.children.map((child) => Math.round(child.top))).size, "one row at a wide page").toBe(1);
    expect(body.children[1]!.left, "the second column sits beside the first").toBeGreaterThan(body.children[0]!.right - 1);
  }
});
