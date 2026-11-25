import { expect, test, type Page } from "@playwright/test";

/**
 * Issue #982, checked on screen: the row an address names draws the designed ring and halo, and the Settings callouts and
 * phone chip row keep their focus rings and their narrow layout. The dev-only mock serves the data; nothing here reads the
 * private designs. The stylesheet rules are covered by src/features/settings/settingsFocusRing.test.ts.
 */

interface AnchorCapture {
  animationName: string;
  animationDuration: string;
  outlineStyle: string;
  boxShadow: string;
  animations: { name: string | undefined; duration: number | undefined }[];
}

const ANCHOR_INIT = () => {
  const seen = window as unknown as { __anchor: AnchorCapture | null };
  seen.__anchor = null;
  const capture = () => {
    const row = document.querySelector('[data-anchored="true"]');
    if (!row || seen.__anchor) return;
    const style = getComputedStyle(row);
    seen.__anchor = {
      animationName: style.animationName,
      animationDuration: style.animationDuration,
      outlineStyle: style.outlineStyle,
      boxShadow: style.boxShadow,
      animations: row.getAnimations().map((animation) => ({
        name: animation instanceof CSSAnimation ? animation.animationName : undefined,
        duration: animation.effect?.getTiming().duration as number | undefined,
      })),
    };
  };
  const watch = () => {
    new MutationObserver(capture).observe(document.documentElement, { attributes: true, subtree: true, attributeFilter: ["data-anchored"] });
  };
  if (document.documentElement) watch();
  else document.addEventListener("readystatechange", () => { if (document.documentElement) watch(); }, { once: true });
};

const anchor = (page: Page) =>
  page.evaluate(() => (window as unknown as { __anchor: AnchorCapture | null }).__anchor);

const openSettings = async (page: Page, hash: string) => {
  await page.goto("/__mock/scenario/default");
  await page.goto(`/#${hash}`);
};

test("a row an address names draws its ring and halo, without the native focus ring", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.addInitScript(ANCHOR_INIT);
  await openSettings(page, "/settings/chat/search-engine");
  const row = page.locator("#chat-search-engine");
  await expect(row).toHaveAttribute("data-anchored", "true");
  await expect(row).toBeFocused();
  // The row really is keyboard-visible, so "outline: none" below is the designed ring replacing a ring, not a vacuous pass.
  await expect(row.evaluate((element) => element.matches(":focus-visible"))).resolves.toBe(true);
  await expect.poll(() => anchor(page)).not.toBeNull();
  const seen = (await anchor(page))!;
  // The native focus ring base.css draws for [tabindex] is gone: the designed ring is this row's focus indicator.
  expect(seen.outlineStyle).toBe("none");
  expect(seen.boxShadow).toContain("rgb(59, 130, 246) 0px 0px 0px 2px inset");
  // The halo: a 500ms animation that fades the 5px outer ring, and it is running when the row is revealed.
  expect(seen.animationName).toBe("set-anchor-halo");
  expect(seen.animationDuration).toBe("0.5s");
  expect(seen.animations).toContainEqual({ name: "set-anchor-halo", duration: 500 });
  // No timer: the ring stays after the halo has faded, and leaves only on the next click or key.
  await page.waitForTimeout(700);
  await expect(row).toHaveAttribute("data-anchored", "true");
  expect(await row.evaluate((element) => getComputedStyle(element).boxShadow)).toContain("2px inset");
});

test("reduced motion leaves the halo out and still marks the row", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addInitScript(ANCHOR_INIT);
  await openSettings(page, "/settings/chat/search-engine");
  const row = page.locator("#chat-search-engine");
  await expect(row).toHaveAttribute("data-anchored", "true");
  await expect.poll(() => anchor(page)).not.toBeNull();
  await expect.poll(() => row.evaluate((element) => getComputedStyle(element).boxShadow)).toContain("rgb(59, 130, 246) 0px 0px 0px 2px inset");
  const seen = (await anchor(page))!;
  expect(seen.animationName).toBe("none");
  expect(seen.animations.some((animation) => animation.name === "set-anchor-halo")).toBe(false);
  expect(seen.outlineStyle).toBe("none");
});

test("a 600px window keeps the back link for a sub-page and rings the row it names", async ({ page }) => {
  await page.setViewportSize({ width: 600, height: 900 });
  await openSettings(page, "/settings/appearance/theme-editor");
  await expect(page.locator(".settings-crumbs-back a")).toHaveText(/Back to Appearance/);
  await expect(page.locator("nav.settings-crumbs")).toHaveCount(0);
  await page.goto("/#/settings/chat/search-engine");
  const row = page.locator("#chat-search-engine");
  await expect(row).toHaveAttribute("data-anchored", "true");
  const measured = await row.evaluate((element) => {
    const style = getComputedStyle(element);
    const rowBounds = element.getBoundingClientRect();
    const scroller = element.closest(".settings-scroll")!.getBoundingClientRect();
    return {
      boxShadow: style.boxShadow,
      ringInside: rowBounds.left >= scroller.left && rowBounds.right <= scroller.right,
    };
  });
  expect(measured.boxShadow).toContain("rgb(59, 130, 246) 0px 0px 0px 2px inset");
  expect(measured.ringInside).toBe(true);
});

test("the phone chip row keeps a tabbed chip's focus ring inside its scroller", async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 768 });
  await openSettings(page, "/settings/general");
  await expect(page.locator(".settings-nav a").first()).toBeVisible();
  const measured = () =>
    page.evaluate(() => {
      const nav = document.querySelector(".settings-nav");
      const active = document.activeElement;
      if (!nav || !(active instanceof HTMLElement) || !active.closest(".settings-nav")) return null;
      const style = getComputedStyle(active);
      const offset = parseFloat(style.outlineOffset);
      const width = parseFloat(style.outlineWidth);
      const chip = active.getBoundingClientRect();
      const bounds = nav.getBoundingClientRect();
      const box = {
        left: bounds.left + nav.clientLeft,
        top: bounds.top + nav.clientTop,
        right: bounds.left + nav.clientLeft + nav.clientWidth,
        bottom: bounds.top + nav.clientTop + nav.clientHeight,
      };
      return {
        text: active.textContent?.trim(),
        outlineStyle: style.outlineStyle,
        overflowY: getComputedStyle(nav).overflowY,
        ring: { left: chip.left - offset - width, top: chip.top - offset - width, right: chip.right + offset + width, bottom: chip.bottom + offset + width },
        box,
      };
    });
  type Measured = NonNullable<Awaited<ReturnType<typeof measured>>>;
  const fits = (one: Measured) =>
    one.ring.left >= one.box.left - 0.01 && one.ring.top >= one.box.top - 0.01 &&
    one.ring.right <= one.box.right + 0.01 && one.ring.bottom <= one.box.bottom + 0.01;
  // Start from the search field: the section row is its next sibling in the tab order, so the first Tab reaches a chip
  // whichever element the route left focused.
  await page.getByRole("searchbox", { name: "Search settings" }).focus();
  await page.keyboard.press("Tab");
  const first = await measured();
  expect(first).not.toBeNull();
  expect(first!.overflowY).toBe("hidden");
  expect(first!.outlineStyle).toBe("solid");
  expect(fits(first!)).toBe(true);
  let last: Measured | null = null;
  for (let press = 0; press < 40 && last === null; press += 1) {
    await page.keyboard.press("Tab");
    const one = await measured();
    if (one?.text === "Archive") last = one;
  }
  expect(last, "keyboard focus reaches the last section chip").not.toBeNull();
  expect(fits(last!)).toBe(true);
});

test("the unknown-page note wraps in a phone window and stays on one row above it", async ({ page }) => {
  await page.setViewportSize({ width: 599, height: 768 });
  await openSettings(page, "/settings/no-such-page");
  await expect(page.locator(".settings-note")).toBeVisible();
  const note = await page.locator(".settings-note").evaluate((element) => {
    const style = getComputedStyle(element);
    const span = element.querySelector("span")!;
    return { flexWrap: style.flexWrap, spanBasis: getComputedStyle(span).flexBasis, spanWidth: span.getBoundingClientRect().width, noteWidth: element.getBoundingClientRect().width };
  });
  expect(note.flexWrap).toBe("wrap");
  expect(note.spanWidth).toBeGreaterThan(note.noteWidth * 0.9);
  await page.setViewportSize({ width: 600, height: 768 });
  expect(await page.locator(".settings-note").evaluate((element) => getComputedStyle(element).flexWrap)).toBe("nowrap");
});
test("forced colors keeps a visible focus outline on the row an address names", async ({ page }) => {
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.emulateMedia({ forcedColors: "active" });
  await openSettings(page, "/settings/chat/search-engine");
  const row = page.locator("#chat-search-engine");
  await expect(row).toHaveAttribute("data-anchored", "true");
  await expect(row).toBeFocused();
  const measured = await row.evaluate((element) => {
    const style = getComputedStyle(element);
    return {
      focusVisible: element.matches(":focus-visible"),
      outlineStyle: style.outlineStyle,
      outlineWidth: style.outlineWidth,
      outlineOffset: style.outlineOffset,
    };
  });
  // The inset box-shadow ring is forced to none, so the row must keep a real outline drawn inside its bounds.
  expect(measured.focusVisible).toBe(true);
  expect(measured.outlineStyle).toBe("solid");
  expect(measured.outlineWidth).toBe("2px");
  expect(measured.outlineOffset).toBe("-2px");
});
