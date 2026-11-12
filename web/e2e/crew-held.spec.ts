import { expect, test, type Page } from "@playwright/test";

/** A held command against the mock: `crew-held` has the sign-in gate off, `crew-held-account…` has a person signed in. */
const PASSWORD = "correct horse battery";
const ORDERS = /DELETE FROM orders/;

async function open(page: Page, scenario: string) {
  await page.goto(`/__mock/scenario/${scenario}`);
  await page.goto("/#/crew/approvals");
  await expect(page.getByText("Needs you · 3")).toBeVisible();
}

const card = (page: Page) => page.getByRole("complementary", { name: ORDERS });

test("approves once after Confirm it's you: wrong password first, then the right one", async ({ page }) => {
  await open(page, "crew-held-account");
  await page.getByRole("table", { name: "Approval log" }).getByRole("button", { name: ORDERS }).click();
  await expect(card(page)).toContainText("Held requests are cleared if Orbyn restarts.");
  await expect(card(page).getByRole("button")).toHaveText(["Approve once", "Deny", "Dismiss"]);
  await card(page).getByRole("button", { name: "Approve once" }).click();
  const dialog = page.getByRole("dialog", { name: "Confirm it’s you" });
  await expect(dialog.getByLabel("Password")).toBeFocused();
  await expect(dialog.getByRole("button", { name: "Approve once" })).toBeDisabled();
  await dialog.getByLabel("Password").fill("not it");
  await dialog.getByRole("button", { name: "Approve once" }).click();
  await expect(dialog.getByText("That isn’t your password. Check it and try again.")).toBeVisible();
  await expect(dialog.getByLabel("Password")).toHaveValue("");
  await dialog.getByLabel("Password").fill(PASSWORD);
  await dialog.getByRole("button", { name: "Approve once" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByText("Needs you · 2")).toBeVisible();
  await expect(page.getByRole("table", { name: "Approval log" }).getByRole("row").filter({ hasText: ORDERS })).toContainText("Approved by you");
});

test("Deny and Dismiss need no password", async ({ page }) => {
  await open(page, "crew-held-account");
  await page.getByRole("button", { name: /DELETE FROM sessions/ }).click();
  await page.getByRole("complementary", { name: /DELETE FROM sessions/ }).getByRole("button", { name: "Deny" }).click();
  await expect(page.getByText("Needs you · 2")).toBeVisible();
  await expect(page.getByRole("dialog")).toHaveCount(0);
  await page.getByRole("button", { name: /aws s3 rb/ }).click();
  await page.getByRole("complementary", { name: /aws s3 rb/ }).getByRole("button", { name: "Dismiss" }).click();
  await expect(page.getByText("Needs you · 1")).toBeVisible();
  await expect(page.getByRole("button", { name: /aws s3 rb/ })).toHaveCount(0);
});

test("without a signed-in account Approve once is off with its reason, and Deny still works", async ({ page }) => {
  await open(page, "crew-held");
  await page.getByRole("button", { name: ORDERS }).click();
  await expect(card(page)).toContainText("Approving needs a signed-in account.");
  await expect(card(page).getByRole("button", { name: "Approve once" })).toBeDisabled();
  await expect(card(page).getByRole("button", { name: "Sign in" })).toBeVisible();
  await card(page).getByRole("button", { name: "Deny" }).click();
  await expect(page.getByText("Needs you · 2")).toBeVisible();
});

test("a request that expired says nothing ran and keeps only Close", async ({ page }) => {
  await open(page, "crew-held-account-expired");
  await page.getByRole("button", { name: ORDERS }).click();
  await card(page).getByRole("button", { name: "Approve once" }).click();
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("dialog").getByRole("button", { name: "Approve once" }).click();
  await expect(card(page)).toContainText("This request expired. Nothing ran.");
  await expect(card(page).getByRole("button")).toHaveText(["Close"]);
  await expect(page.getByText("Needs you · 2")).toBeVisible();
});

test("a request decided elsewhere says it was denied, without claiming who did", async ({ page }) => {
  await open(page, "crew-held-account-race");
  await page.getByRole("button", { name: ORDERS }).click();
  await card(page).getByRole("button", { name: "Deny" }).click();
  await expect(card(page)).toContainText("Already decided.");
  await expect(card(page)).toContainText("Denied. Nothing ran.");
  await expect(page.getByText("Needs you · 2")).toBeVisible();
});

test("the card fits a 320 px panel without a sideways scroll", async ({ page }) => {
  await page.setViewportSize({ width: 320, height: 700 });
  await open(page, "crew-held-account");
  await page.getByRole("button", { name: ORDERS }).click();
  await expect(card(page).getByRole("button", { name: "Dismiss" })).toBeVisible();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});
