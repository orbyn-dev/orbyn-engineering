import { expect, test, type Page } from "@playwright/test";
import { createServer, type IncomingMessage, type Server, type ServerResponse } from "node:http";
import { existsSync } from "node:fs";
import { join } from "node:path";
import { openPanelTab, startProject, type Service } from "./service";

/** H36's typed size, aspect-ratio lock, handles after the toolbar is hidden and narrow two-row toolbar, against the real orbynd. */
const SITE_PORT = 5295;

let service: Service | undefined;
let threadId = "";
let site: Server | undefined;
let site6: Server | undefined;

/** The service the tests talk to; beforeAll starts it before any test runs. */
function real(): Service {
  if (!service) throw new Error("The real service did not start.");
  return service;
}

/** Closes one of the fixture's loopback servers, when it was opened. */
const close = (server: Server | undefined) =>
  server?.listening
    ? new Promise<void>((resolve, reject) => server.close((error) => (error ? reject(error) : resolve())))
    : Promise.resolve();

test.beforeAll(async () => {
  const handler = (request: IncomingMessage, response: ServerResponse) => {
    response.setHeader("Content-Type", "text/html");
    response.end(`<body style="margin:0;background:#2a6"><button style="width:100vw;height:100vh">${request.url}</button></body>`);
  };
  // The preview browser resolves localhost to ::1 or 127.0.0.1, whichever it tries first, so answer on both.
  const listen = (server: Server, host: string) =>
    new Promise<void>((resolve, reject) => {
      server.once("error", reject);
      server.listen(SITE_PORT, host, () => resolve());
    });
  const loopback = createServer(handler);
  await listen(loopback, "127.0.0.1");
  site = loopback;
  const loopback6 = createServer(handler);
  await listen(loopback6, "::1").catch(() => {
    loopback6.close(() => {});
  });
  site6 = loopback6.listening ? loopback6 : undefined;
  const started = await startProject({ fakeEngines: true });
  service = started.service;
  const thread = await service.api<{ id: string }>("POST", "/api/threads", {
    project_id: started.projectId,
    engine: started.engines[0]?.id,
    model: null,
    title: "Device toolbar",
    worktree: null,
    skill_profile: null,
  });
  threadId = thread.id;
});

test.afterAll(async () => {
  const failures: unknown[] = [];
  // End the previews' browsers before the service exits, so Windows can release their profiles. Every owner is
  // cleaned up even when another one fails, so a refused DELETE never leaves the service or a port behind.
  try {
    const current = service;
    if (current && threadId) {
      const base = `/api/threads/${threadId}/previews`;
      for (const tab of await current.api<{ id: string }[]>("GET", base)) {
        await current.api("DELETE", `${base}/${tab.id}`);
        await expect.poll(() => existsSync(join(current.dataDir, "preview-profiles", tab.id))).toBe(false);
      }
    }
  } catch (failure) {
    failures.push(failure);
  }
  const settled = await Promise.allSettled([service?.stop(), close(site), close(site6)]);
  for (const result of settled) if (result.status === "rejected") failures.push(result.reason as unknown);
  if (failures.length > 0) throw new AggregateError(failures, "The device toolbar fixture did not tear down cleanly.");
});

/** The preview tab as the service keeps it, which is what a resize has to reach. */
const tab = async () =>
  (await real().api<{ width: number; height: number }[]>("GET", `/api/threads/${threadId}/previews`))[0];

/** A resize the toolbar sends, awaited before the act that sends it. */
const nextResize = (page: Page) =>
  page.waitForResponse((response) =>
    response.request().method() === "POST" &&
    response.url().includes(`/api/threads/${threadId}/previews/`) &&
    response.url().endsWith("/resize"),
  );

/** Every resize the page sent, so a commit that should not happen is seen as one. */
function watchResizes(page: Page) {
  const sent: string[] = [];
  page.on("request", (request) => {
    if (request.method() === "POST" && request.url().endsWith("/resize")) sent.push(request.url());
  });
  return sent;
}

const widthField = (page: Page) => page.getByRole("spinbutton", { name: "Width" });
const heightField = (page: Page) => page.getByRole("spinbutton", { name: "Height" });
const lockButton = (page: Page) => page.getByRole("button", { name: "Lock aspect ratio" });
const deviceToolbar = (page: Page) => page.getByRole("toolbar", { name: "Device toolbar" });
const deviceSelect = (page: Page) => page.getByRole("combobox", { name: "Device" });

/** Opens the Browser panel on the loopback page and puts the toolbar in the phone preset (393 by 852). */
async function openPhoneToolbar(page: Page) {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto(`${real().page}#/threads/${threadId}`);
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
  await openPanelTab(page, "Browser");
  const address = page.getByRole("combobox", { name: "Search or enter URL" });
  await address.fill(`localhost:${SITE_PORT}/one`);
  await address.press("Enter");
  await expect(page.getByRole("img", { name: /^Page at/ })).toBeVisible({ timeout: 40_000 });
  await page.getByRole("button", { name: "More page actions" }).click();
  await page.getByRole("menuitem", { name: "Show device toolbar" }).click();
  await deviceSelect(page).selectOption("phone");
  await expect(widthField(page)).toHaveValue("393", { timeout: 20_000 });
  await expect(deviceToolbar(page)).toBeVisible();
}

test("types a width and a height, one resize on Enter and one when the field is left", async ({ page }) => {
  test.setTimeout(120_000);
  const resizes = watchResizes(page);
  await openPhoneToolbar(page);
  const before = resizes.length;

  await widthField(page).fill("500");
  expect(resizes).toHaveLength(before);
  const onEnter = nextResize(page);
  await widthField(page).press("Enter");
  expect((await onEnter).ok()).toBe(true);
  expect(JSON.parse((await onEnter).request().postData() ?? "{}")).toEqual({ width: 500, height: 852 });
  await expect.poll(async () => (await tab())?.width).toBe(500);
  await expect(widthField(page)).toHaveValue("500");
  await expect(heightField(page)).toHaveValue("852");
  // A typed size is no preset any more, as after a drag.
  await expect(deviceSelect(page)).toHaveValue("custom");
  await expect(page.getByRole("option", { name: "Custom" })).toBeAttached();

  await heightField(page).fill("700");
  expect(resizes).toHaveLength(before + 1);
  const onLeave = nextResize(page);
  await heightField(page).press("Tab");
  expect((await onLeave).ok()).toBe(true);
  expect(JSON.parse((await onLeave).request().postData() ?? "{}")).toEqual({ width: 500, height: 700 });
  await expect.poll(async () => (await tab())?.height).toBe(700);
  expect(await tab()).toMatchObject({ width: 500, height: 700 });
  await expect(heightField(page)).toHaveValue("700");
  expect(resizes).toHaveLength(before + 2);
});

test("turns a size outside 100 to 4096 red, sends nothing, and puts the previous number back", async ({ page }) => {
  test.setTimeout(120_000);
  const resizes = watchResizes(page);
  await openPhoneToolbar(page);
  const before = resizes.length;

  await widthField(page).fill("80");
  await expect(widthField(page)).toHaveAttribute("aria-invalid", "true");
  await expect(page.getByRole("alert")).toHaveText("Sizes run from 100 to 4096 pixels.");
  await widthField(page).press("Enter");
  await expect(widthField(page)).toHaveValue("393");
  await expect(page.getByRole("alert")).toHaveCount(0);

  await widthField(page).fill("5000");
  await expect(widthField(page)).toHaveAttribute("aria-invalid", "true");
  await widthField(page).press("Escape");
  await expect(widthField(page)).toHaveValue("393");

  await heightField(page).fill("99");
  await expect(page.getByRole("alert")).toHaveText("Sizes run from 100 to 4096 pixels.");
  await heightField(page).press("Tab");
  await expect(heightField(page)).toHaveValue("852");

  expect(resizes).toHaveLength(before);
  expect(await tab()).toMatchObject({ width: 393, height: 852 });
});

test("keeps the locked ratio for typing, a dragged edge and Rotate, and forgets the lock when the toolbar closes", async ({ page }) => {
  test.setTimeout(120_000);
  const resizes = watchResizes(page);
  await openPhoneToolbar(page);
  const before = resizes.length;

  await lockButton(page).click();
  await expect(lockButton(page)).toHaveAttribute("aria-pressed", "true");
  await widthField(page).fill("500");
  const typed = nextResize(page);
  await widthField(page).press("Enter");
  expect(JSON.parse((await typed).request().postData() ?? "{}")).toEqual({ width: 500, height: 1084 });
  await expect.poll(async () => (await tab())?.height).toBe(1084);
  expect(await tab()).toMatchObject({ width: 500, height: 1084 });
  await expect(widthField(page)).toHaveValue("500");
  await expect(heightField(page)).toHaveValue("1084");

  // Rotate swaps the numbers, and the ratio with them.
  const rotated = nextResize(page);
  await page.getByRole("button", { name: "Rotate" }).click();
  expect(JSON.parse((await rotated).request().postData() ?? "{}")).toEqual({ width: 1084, height: 500 });
  await expect.poll(async () => (await tab())?.width).toBe(1084);
  expect(await tab()).toMatchObject({ width: 1084, height: 500 });
  await expect(widthField(page)).toHaveValue("1084");
  await expect(heightField(page)).toHaveValue("500");

  // A dragged edge keeps the locked ratio too.
  const handle = page.getByRole("button", { name: /Resize the page from the right edge/ });
  const box = await handle.boundingBox();
  if (!box) throw new Error("the right edge has no box to drag");
  const dragged = nextResize(page);
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await expect(handle).toHaveAttribute("data-dragging", "true");
  await page.mouse.move(box.x + box.width / 2 + 50, box.y + box.height / 2, { steps: 8 });
  await page.mouse.up();
  const moved = JSON.parse((await dragged).request().postData() ?? "{}") as { width: number; height: number };
  expect(moved.width).toBeGreaterThan(1084);
  expect(moved.height).toBe(Math.min(4096, Math.max(100, Math.round((moved.width * 393) / 852))));
  await expect.poll(async () => (await tab())?.width).toBe(moved.width);
  expect(await tab()).toMatchObject({ width: moved.width });
  expect(resizes.length).toBe(before + 3);

  // The lock is not remembered: it is off again when the toolbar is closed.
  await page.getByRole("button", { name: "More page actions" }).click();
  await page.getByRole("menuitem", { name: "Hide device toolbar" }).click();
  await expect(deviceToolbar(page)).toHaveCount(0);
  await page.getByRole("button", { name: /Show device toolbar/ }).click();
  await expect(deviceToolbar(page)).toBeVisible();
  await expect(lockButton(page)).toHaveAttribute("aria-pressed", "false");
});

test("keeps the handles after the toolbar is hidden, and the pill brings the toolbar back", async ({ page }) => {
  test.setTimeout(120_000);
  const resizes = watchResizes(page);
  await openPhoneToolbar(page);
  const before = resizes.length;

  await page.getByRole("button", { name: "More page actions" }).click();
  await page.getByRole("menuitem", { name: "Hide device toolbar" }).click();
  await expect(deviceToolbar(page)).toHaveCount(0);
  // The page keeps its box, its handles and the size the preset gave it.
  await expect(page.locator(".browser-box")).toBeVisible();
  await expect(page.getByRole("button", { name: /Resize the page/ })).toHaveCount(5);
  const pill = page.getByRole("button", { name: /Show device toolbar/ });
  await expect(pill).toHaveText("393 by 852 · Show device toolbar");
  expect(resizes).toHaveLength(before);

  // The handles still resize the page with the toolbar away.
  const handle = page.getByRole("button", { name: /Resize the page from the bottom-right corner/ });
  const box = await handle.boundingBox();
  if (!box) throw new Error("the bottom-right corner has no box to drag");
  const corner = nextResize(page);
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 30, box.y + box.height / 2 + 20, { steps: 8 });
  await page.mouse.up();
  const moved = JSON.parse((await corner).request().postData() ?? "{}") as { width: number; height: number };
  expect(moved.width).toBeGreaterThan(393);
  expect(moved.height).toBeGreaterThan(852);
  await expect.poll(async () => (await tab())?.width).toBe(moved.width);

  // The pill names the new size and brings the toolbar back.
  await expect(pill).toHaveText(`${moved.width} by ${moved.height} · Show device toolbar`);
  await pill.click();
  await expect(deviceToolbar(page)).toBeVisible();
  await expect(pill).toHaveCount(0);
  await expect(widthField(page)).toHaveValue(String(moved.width));
  await expect(heightField(page)).toHaveValue(String(moved.height));
  await expect(deviceSelect(page)).toHaveValue("custom");
});

test("shares one row at the default panel and puts the device choice above the fields in a 320 px panel", async ({ page }) => {
  test.setTimeout(120_000);
  await openPhoneToolbar(page);
  const panel = page.getByRole("complementary", { name: "Thread context" });
  expect((await panel.boundingBox())?.width).toBe(380);

  /** The toolbar's boxes, as the browser lays them out. */
  const measure = () =>
    deviceToolbar(page).evaluate((toolbar) => {
      const rect = (element: Element) => {
        const box = element.getBoundingClientRect();
        return { top: box.top, bottom: box.bottom, left: box.left, right: box.right, width: box.width, height: box.height };
      };
      const select = toolbar.querySelector("select");
      const sizes = toolbar.querySelector(".browser-devices-sizes");
      if (!select || !sizes) throw new Error("the device toolbar has no select and sizes row");
      const children = [...sizes.children].map(rect);
      const centres = children.map((child) => (child.top + child.bottom) / 2);
      const centre = (box: { top: number; bottom: number }) => (box.top + box.bottom) / 2;
      return {
        toolbar: rect(toolbar),
        select: rect(select),
        sizes: rect(sizes),
        children,
        widestCentreSpread: Math.max(...centres) - Math.min(...centres),
        rowCentreGap: Math.abs(centre(rect(select)) - centre(rect(sizes))),
      };
    });

  // At the 380 px panel the choice and the sizes share one row (pb-h36-fields).
  const wide = await measure();
  expect(wide.rowCentreGap, JSON.stringify(wide)).toBeLessThan(1);
  expect(wide.widestCentreSpread, JSON.stringify(wide)).toBeLessThan(1);

  // Drag the panel's edge in to the narrowest width the frame draws: 320 px.
  const separator = page.getByRole("separator", { name: "Resize panel" });
  const box = await separator.boundingBox();
  if (!box) throw new Error("the panel edge has no box to drag");
  await page.mouse.move(box.x + box.width / 2, box.y + 300);
  await page.mouse.down();
  await page.mouse.move(box.x + box.width / 2 + 140, box.y + 300, { steps: 14 });
  await page.mouse.up();
  await expect.poll(async () => (await panel.boundingBox())?.width).toBe(320);

  // The frame's two rows: the device choice alone, then the fields, the lock and Rotate together.
  const narrow = await measure();
  expect(narrow.select.bottom, JSON.stringify(narrow)).toBeLessThanOrEqual(narrow.sizes.top);
  expect(narrow.widestCentreSpread, JSON.stringify(narrow)).toBeLessThan(1);
  expect(narrow.children, JSON.stringify(narrow)).toHaveLength(5);
  expect(narrow.select.left, JSON.stringify(narrow)).toBeGreaterThanOrEqual(narrow.toolbar.left);
  expect(narrow.select.right, JSON.stringify(narrow)).toBeLessThanOrEqual(narrow.toolbar.right);
  expect(narrow.sizes.left, JSON.stringify(narrow)).toBeGreaterThanOrEqual(narrow.toolbar.left);
  expect(narrow.sizes.right, JSON.stringify(narrow)).toBeLessThanOrEqual(narrow.toolbar.right);
  // The fields keep their own size, and the page keeps the size it was given instead of refitting.
  expect(narrow.children[0]?.width, JSON.stringify(narrow)).toBeLessThanOrEqual(64);
  await expect(widthField(page)).toHaveValue("393");
  expect(await tab()).toMatchObject({ width: 393, height: 852 });
});
