import { expect, test } from "@playwright/test";

/**
 * The trust prompt's Path row, against the designed frame `trust` of engineer-designs.html: the facts card is the design's
 * `.card` (padding 12px 14px) and the row its `.kv` (label at the start, value at the end). The value is `.kv b`, which
 * carries no `text-align`, so a path long enough to wrap keeps one left edge instead of ragging line by line against the
 * right one — the bug in #1117, which right-aligned every wrapped line and broke the folder name mid-token.
 *
 * The fixture's untrusted folder (`mock/fixtures.ts`, `p-new`) has a folder name longer than the 480px dialog's value
 * column at the 1024px window the issue reports, so it wraps.
 */
test("the trust prompt's path wraps with one left edge, inside the row the design draws", async ({ page }) => {
  await page.setViewportSize({ width: 1024, height: 800 });
  await page.goto("/#/trust/p-new");

  const path = page.locator(".trust-path");
  // The dev server compiles the app on its first request; a cold run on Windows can outrun the 10 s expect timeout.
  await expect(path).toBeVisible({ timeout: 30_000 });
  await expect(path).toHaveText("~/code/clients/acme/vendor-sdk-with-a-rather-long-repository-name-for-layout");

  // A wrapping row is what makes the alignment observable at all.
  const lines = await path.evaluate((el) => {
    const range = document.createRange();
    range.selectNodeContents(el);
    return Array.from(range.getClientRects()).map((rect) => ({ left: Math.round(rect.left), right: Math.round(rect.right) }));
  });
  expect(lines.length, "the path must wrap for this assertion to mean anything").toBeGreaterThan(1);
  // Design `.kv b` has no text-align: every wrapped line starts at the same edge (it is a left edge, not a right one).
  const lefts = [...new Set(lines.map((line) => line.left))];
  expect(lefts, `wrapped lines start at ${lefts.join(", ")}`).toHaveLength(1);

  const geometry = await page.evaluate(() => {
    const facts = document.querySelector(".trust-facts") as HTMLElement;
    const row = facts.querySelector("div") as HTMLElement;
    const label = facts.querySelector("dt") as HTMLElement;
    const value = facts.querySelector("dd") as HTMLElement;
    const box = (el: HTMLElement) => Math.round(el.getBoundingClientRect().right);
    const start = (el: HTMLElement) => Math.round(el.getBoundingClientRect().left);
    return { factsStart: start(facts), factsEnd: box(facts), labelStart: start(label), rowEnd: box(row), valueStart: start(value), valueEnd: box(value) };
  });
  // The design's `.card` padding is 12px 14px, so the label starts and the value ends 14px inside the card.
  expect(geometry.labelStart - geometry.factsStart, "label inset breaks the .card padding").toBe(14);
  expect(geometry.factsEnd - geometry.rowEnd, "row does not fill the card's content box").toBe(14);
  // `.kv` puts the label at the start and the value at the end of the row.
  expect(geometry.rowEnd - geometry.valueEnd).toBe(0);
  // The value keeps a column of its own beside the label instead of running under it.
  expect(geometry.valueStart).toBeGreaterThan(geometry.labelStart);
});
