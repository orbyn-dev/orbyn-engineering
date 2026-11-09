import { expect, test, type Locator, type Page } from "@playwright/test";

/**
 * Characterization of dragging a thread row (Sidebar.tsx onPressStart, dragDrop.ts): onto the composer it
 * becomes a thread link; in Ledger a row can only be placed inside its own state section. Both rest on
 * getBoundingClientRect, so the drags are real mouse events in a real layout.
 */

const tree = (page: Page) => page.locator(".sidebar-scroll");
const row = (page: Page, title: string) => tree(page).locator("li[data-thread-item]", { hasText: title }).locator("a.thread-row");
const composer = (page: Page) => page.getByRole("textbox", { name: /^Message/ });

const centre = async (target: Locator) => {
  const box = await target.boundingBox();
  if (!box) throw new Error("no box");
  // Left of centre: clear of the row's trailing action buttons.
  return { x: box.x + 40, y: box.y + box.height / 2, box };
};

/**
 * Presses on a row and moves just past the drag threshold; the pointer stays down. It then waits for proof that
 * the press became a drag: trackPress's window listener calls preventDefault on every move once the drag is live,
 * and this probe, added after it, sees the event already prevented. (The composer's own data-dragging is the HTML5
 * dragover state, which a pointer drag never sets, so it cannot serve as the signal.)
 */
async function pickUp(page: Page, target: Locator) {
  const at = await centre(target);
  await page.mouse.move(at.x, at.y);
  await page.mouse.down();
  await page.evaluate(() => {
    const probe = window as unknown as { __dragLive: boolean };
    probe.__dragLive = false;
    window.addEventListener("pointermove", (event) => void (probe.__dragLive = event.defaultPrevented));
  });
  await page.mouse.move(at.x, at.y + 10, { steps: 3 });
  await expect.poll(() => page.evaluate(() => (window as unknown as { __dragLive: boolean }).__dragLive)).toBe(true);
}

/** Two frames and a macrotask: whatever a release set in motion has run, so a "nothing was saved" check is not early. */
const settled = (page: Page) =>
  page.evaluate(() => new Promise<void>((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 0)))));

const drawnIds = (page: Page) =>
  tree(page).locator("li[data-thread-item]").evaluateAll((items) => items.map((item) => (item as HTMLElement).dataset.threadItem ?? ""));

/** Records every save of the thread order and answers it, so a drop's effect does not depend on the mock. */
async function recordOrderSaves(page: Page) {
  const saved: string[][] = [];
  await page.route("**/api/threads/order", async (route) => {
    saved.push((route.request().postDataJSON() as { ids: string[] }).ids);
    await route.fulfill({ status: 204 });
  });
  return saved;
}

test.describe("onto the composer", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/threads/t-done");
    await expect(row(page, "Tool call states")).toBeVisible();
  });

  const dropOnComposer = async (page: Page, title: string) => {
    await pickUp(page, row(page, title));
    const box = await page.locator(".composer").boundingBox();
    if (!box) throw new Error("no composer box");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 12 });
    await page.mouse.up();
  };

  test("a dropped row becomes a thread link in the draft, and the route stays", async ({ page }) => {
    await expect(composer(page)).toHaveValue("");
    await dropOnComposer(page, "Tool call states");
    await expect(composer(page)).toHaveValue("[Tool call states](orbyn://thread/t-tools) ");
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
  });

  test("the link is appended after what is already typed", async ({ page }) => {
    await composer(page).fill("compare with  ");
    await dropOnComposer(page, "Tool call states");
    await expect(composer(page)).toHaveValue("compare with [Tool call states](orbyn://thread/t-tools) ");
  });

  test("a multi-selection drops as one link per thread, in drawn order", async ({ page }) => {
    await row(page, "Explain how the event log resumes").click({ modifiers: ["ControlOrMeta"] });
    await row(page, "Tool call states").click({ modifiers: ["ControlOrMeta"] });
    await expect(tree(page).locator("li[data-selected]")).toHaveCount(2);
    await dropOnComposer(page, "Tool call states");
    await expect(composer(page)).toHaveValue(
      "[Tool call states](orbyn://thread/t-tools) [Explain how the event log resumes](orbyn://thread/t-done) ",
    );
  });

  test("a drop anywhere else changes nothing", async ({ page }) => {
    const saved = await recordOrderSaves(page);
    const before = await drawnIds(page);
    await pickUp(page, row(page, "Tool call states"));
    const target = await centre(row(page, "Upgrade tokio"));
    await page.mouse.move(target.x, target.y, { steps: 10 });
    await page.mouse.up();
    await settled(page);
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
    await expect(composer(page)).toHaveValue("");
    expect(await drawnIds(page)).toEqual(before);
    expect(saved).toEqual([]);
  });

  test("Escape during the drag cancels it", async ({ page }) => {
    await pickUp(page, row(page, "Tool call states"));
    await page.keyboard.press("Escape");
    const box = await page.locator(".composer").boundingBox();
    if (!box) throw new Error("no composer box");
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2, { steps: 8 });
    await page.mouse.up();
    await settled(page);
    await expect(composer(page)).toHaveValue("");
    await expect(page).toHaveURL(/#\/threads\/t-done$/);
  });

  // Current behaviour, not necessarily desired: folder view reserves the drag for the composer.
  test("project folders keep a drag to the composer: rows are not reordered", async ({ page }) => {
    const saved = await recordOrderSaves(page);
    const before = await drawnIds(page);
    // pickUp has already shown the drag is live.
    await pickUp(page, row(page, "Tool call states"));
    const target = await centre(row(page, "Upgrade tokio"));
    await page.mouse.move(target.x, target.y, { steps: 10 });
    // No drop markers and no drag state while dragging in a folder view.
    await expect(tree(page).locator(".drag-boundary")).toHaveCount(0);
    await expect(tree(page)).not.toHaveAttribute("data-dragging");
    await page.mouse.up();
    await settled(page);
    expect(await drawnIds(page)).toEqual(before);
    expect(saved).toEqual([]);
  });
});

test.describe("Ledger", () => {
  test.beforeEach(async ({ page }) => {
    // Tall enough that every section is on screen at once.
    await page.setViewportSize({ width: 1280, height: 1600 });
    await page.addInitScript(() => {
      localStorage.setItem("orbyn.prefs.migrated", "1");
      localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ workspaceLayout: "ledger" }));
    });
    await page.goto("/__mock/scenario/default");
    await page.goto("/#/threads/t-done");
    // Ledger sorts by what needs the person: Needs you, Failed, Running, Done today.
    await expect(tree(page).locator(".ledger-head")).toHaveText([/^Needs you/, /^Failed/, /^Running/, /^Done today/]);
  });

  const dragging = (page: Page) => tree(page).and(page.locator("[data-dragging]"));

  test("a row dropped inside its own section is reordered there", async ({ page }) => {
    const saved = await recordOrderSaves(page);
    expect((await drawnIds(page)).slice(0, 2)).toEqual(["t-approval", "t-site-approval"]);
    await pickUp(page, row(page, "Add approval timeout handling"));
    await expect(dragging(page)).toHaveCount(1);
    // The second Needs row, lower half: the drop slot is after it.
    const over = await centre(row(page, "Push a preview of the pricing page"));
    await page.mouse.move(over.x, over.box.y + over.box.height * 0.85, { steps: 12 });
    // The list shows the move while the pointer is still down.
    await expect.poll(async () => {
      const ids = await drawnIds(page);
      return ids.indexOf("t-site-approval") < ids.indexOf("t-approval");
    }).toBe(true);
    await page.mouse.up();
    // The mock keeps no order, so the 4 s poll would put the old one back: read the result right away.
    await expect(dragging(page)).toHaveCount(0);
    expect((await drawnIds(page)).slice(0, 2)).toEqual(["t-site-approval", "t-approval"]);
    await expect.poll(() => saved.length).toBe(1);
    expect(saved[0]?.slice(0, 2)).toEqual(["t-site-approval", "t-approval"]);
  });

  test("a row cannot be placed in another section: nothing moves, nothing is saved", async ({ page }) => {
    const saved = await recordOrderSaves(page);
    const before = await drawnIds(page);
    await pickUp(page, row(page, "Add approval timeout handling"));
    await expect(dragging(page)).toHaveCount(1);
    // Over a Done row, lower half: not a slot a Needs row may take.
    const over = await centre(row(page, "Tool call states"));
    await page.mouse.move(over.x, over.box.y + over.box.height * 0.85, { steps: 12 });
    // Give the move a frame to render, then the list must still be as it was.
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
    expect(await drawnIds(page)).toEqual(before);
    await page.mouse.up();
    await expect(dragging(page)).toHaveCount(0);
    await settled(page);
    expect(await drawnIds(page)).toEqual(before);
    expect(saved).toEqual([]);
  });

  test("a Failed row cannot be dragged up into Needs you", async ({ page }) => {
    const saved = await recordOrderSaves(page);
    const before = await drawnIds(page);
    await pickUp(page, row(page, "Upgrade tokio and fix the fallout"));
    await expect(dragging(page)).toHaveCount(1);
    const over = await centre(row(page, "Add approval timeout handling"));
    await page.mouse.move(over.x, over.box.y + over.box.height * 0.2, { steps: 12 });
    await page.evaluate(() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(done))));
    expect(await drawnIds(page)).toEqual(before);
    await page.mouse.up();
    await expect(dragging(page)).toHaveCount(0);
    await settled(page);
    expect(await drawnIds(page)).toEqual(before);
    expect(saved).toEqual([]);
  });
});
