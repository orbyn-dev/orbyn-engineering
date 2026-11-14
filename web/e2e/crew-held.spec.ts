import { expect, test, type Page } from "@playwright/test";

/**
 * A held command against the mock. Approving once is the desktop app's native dialog, which a browser cannot show, so a spec fakes the
 * shell's side in the page: the service says it started the app, and the window's bridge answers `crew_presence` as a person would
 * (a random nonce) or as the shell refuses (a code). The clicks are real, so `isTrusted` is the browser's own.
 */
const ORDERS = /DELETE FROM orders/;

type Seen = { calls: [string, Record<string, unknown> | undefined][]; clicks: { detail: number; trusted: boolean }[] };

async function open(page: Page, scenario: string, shell: "app" | "browser" | { refuse: string } = "app") {
  await page.route("**/api/shell", (route) => route.fulfill({ json: { attached: shell !== "browser", windows: 1, facts: {} } }));
  await page.addInitScript((how) => {
    const seen = { calls: [] as unknown[], clicks: [] as unknown[] };
    (window as unknown as { __seen: unknown }).__seen = seen;
    window.addEventListener("click", (event) => seen.clicks.push({ detail: event.detail, trusted: event.isTrusted }), true);
    if (how === "browser") return;
    (window as unknown as { __TAURI_INTERNALS__: unknown }).__TAURI_INTERNALS__ = {
      invoke: async (command: string, args?: Record<string, unknown>) => {
        seen.calls.push([command, args]);
        if (command !== "crew_presence") return undefined;
        if (typeof how === "object") throw how.refuse;
        return Array.from(crypto.getRandomValues(new Uint8Array(16)), (byte) => byte.toString(16).padStart(2, "0")).join("");
      },
    };
  }, shell);
  await page.goto(`/__mock/scenario/${scenario}`);
  await page.goto("/#/crew/approvals");
  await expect(page.getByText("Needs you · 3")).toBeVisible();
}

const seen = (page: Page) => page.evaluate(() => (window as unknown as { __seen: unknown }).__seen as Seen);

const card = (page: Page) => page.getByRole("complementary", { name: ORDERS });

async function openCard(page: Page) {
  await page.getByRole("table", { name: "Approval log" }).getByRole("button", { name: ORDERS }).click();
  await expect(card(page)).toContainText("Held requests are cleared if Orbyn restarts.");
}

test("a click on Approve once asks the shell once with the hold id, then the card says Approved by you", async ({ page }) => {
  await open(page, "crew-held");
  await openCard(page);
  await expect(card(page).getByRole("button")).toHaveText(["Approve once", "Deny", "Dismiss"]);
  await card(page).getByRole("button", { name: "Approve once" }).click();
  await expect(page.getByText("Needs you · 2")).toBeVisible();
  await expect(page.getByRole("table", { name: "Approval log" }).getByRole("row").filter({ hasText: ORDERS })).toContainText("Approved by you");
  const { calls, clicks } = await seen(page);
  expect(calls).toEqual([["crew_presence", { holdId: "hold-orders" }]]);
  // The browser's own click on a mouse press is trusted and counts a press.
  expect(clicks.some((click) => click.trusted && click.detail >= 1)).toBe(true);
  await page.getByRole("button", { name: ORDERS }).click();
  await expect(card(page)).toContainText("Already decided.");
  await expect(card(page)).toContainText("Approved by you");
  await expect(card(page).getByRole("button")).toHaveText(["Close"]);
});

test("a click a script makes calls nothing, and neither does a key on the card", async ({ page }) => {
  await open(page, "crew-held");
  await openCard(page);
  const button = card(page).getByRole("button", { name: "Approve once" });
  await button.evaluate((element) => {
    element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    (element as HTMLButtonElement).click();
  });
  await card(page).focus();
  await page.keyboard.press("Enter");
  await page.keyboard.press("Space");
  await card(page).press("Enter");
  expect((await seen(page)).calls).toEqual([]);
  await expect(card(page)).toContainText("Held for you");
});

test("pressing the focused button with the keyboard is a trusted click with detail 0, which is allowed (a keyboard user's own action)", async ({ page }) => {
  await open(page, "crew-held");
  await openCard(page);
  await card(page).getByRole("button", { name: "Approve once" }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByText("Needs you · 2")).toBeVisible();
  const { calls, clicks } = await seen(page);
  expect(calls).toHaveLength(1);
  // How a keyboard press reports, measured here in Chromium: a real mouse press has detail 1 or more.
  expect(clicks.filter((click) => click.trusted).map((click) => click.detail)).toContain(0);
});

test("a refusal from the shell says it could not confirm it was you, the card stays held and nothing is retried", async ({ page }) => {
  await open(page, "crew-held", { refuse: "presence_refused:mismatch" });
  await openCard(page);
  await card(page).getByRole("button", { name: "Approve once" }).click();
  await expect(card(page)).toContainText("Orbyn couldn’t confirm it was you. Try again from the desktop app.");
  await expect(card(page)).toContainText("Nothing ran. The command is still held.");
  await expect(card(page).getByRole("button")).toHaveText(["Approve once", "Deny", "Dismiss"]);
  await page.waitForTimeout(300);
  expect((await seen(page)).calls).toHaveLength(1);
  await expect(page.getByText("Needs you · 3")).toBeVisible();
});

test("cancelling the dialog says nothing", async ({ page }) => {
  await open(page, "crew-held", { refuse: "cancelled" });
  await openCard(page);
  await card(page).getByRole("button", { name: "Approve once" }).click();
  await expect.poll(async () => (await seen(page)).calls.length).toBe(1);
  await expect(card(page).getByRole("button", { name: "Approve once" })).toBeEnabled();
  await expect(card(page)).not.toContainText("couldn’t confirm");
  await expect(card(page)).toContainText("Why it is held");
});

test("in a browser tab Approve once is not offered, and Deny works", async ({ page }) => {
  await open(page, "crew-held", "browser");
  await openCard(page);
  await expect(card(page)).toContainText("Approving once needs the Orbyn desktop app.");
  await expect(card(page)).toContainText("Deny and Dismiss still work here.");
  await expect(card(page).getByRole("button")).toHaveText(["Deny", "Dismiss"]);
  await card(page).getByRole("button", { name: "Deny" }).click();
  await expect(page.getByText("Needs you · 2")).toBeVisible();
});

test("Deny and Dismiss need no confirmation", async ({ page }) => {
  await open(page, "crew-held");
  await page.getByRole("button", { name: /DELETE FROM sessions/ }).click();
  await page.getByRole("complementary", { name: /DELETE FROM sessions/ }).getByRole("button", { name: "Deny" }).click();
  await expect(page.getByText("Needs you · 2")).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByRole("button", { name: /aws s3 rb/ }).click();
  await page.getByRole("complementary", { name: /aws s3 rb/ }).getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByText("Needs you · 1")).toBeVisible();
  await expect(page.getByRole("button", { name: /aws s3 rb/ })).toHaveCount(0);
});

test("a request that expired says nothing ran and keeps only Close", async ({ page }) => {
  await open(page, "crew-held-expired");
  await page.getByRole("button", { name: ORDERS }).click();
  await card(page).getByRole("button", { name: "Approve once" }).click();
  await expect(card(page)).toContainText("This request expired. Nothing ran.");
  await expect(card(page).getByRole("button")).toHaveText(["Close"]);
  await expect(page.getByText("Needs you · 2")).toBeVisible();
});

test("a request decided elsewhere says it was denied, without claiming who did", async ({ page }) => {
  await open(page, "crew-held-race");
  await page.getByRole("button", { name: ORDERS }).click();
  await card(page).getByRole("button", { name: "Deny" }).click();
  await expect(card(page)).toContainText("Already decided.");
  await expect(card(page)).toContainText("Denied. Nothing ran.");
  await expect(page.getByText("Needs you · 2")).toBeVisible();
});

test("the card fits a 320 px panel without a sideways scroll", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 700 });
  await open(page, "crew-held");
  await page.getByRole("button", { name: ORDERS }).click();
  await expect(card(page).getByRole("button", { name: "Dismiss" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
