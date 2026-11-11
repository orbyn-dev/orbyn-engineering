import { expect, test, type Page } from "@playwright/test";

/** The account against the mock's throwaway stand-in; the default scenario has the sign-in gate off. */
const PASSWORD = "correct horse battery";

async function open(page: Page, scenario: string, hash = "#/settings") {
  await page.goto(`/__mock/scenario/${scenario}`);
  await page.goto(`/${hash}`);
}

test("with the gate off there is no Account in Settings and the footer is the plain Settings row", async ({ page }) => {
  await open(page, "default");
  await expect(page.getByRole("link", { name: "General" })).toBeVisible();
  await expect(page.getByRole("link", { name: "Account", exact: true })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Account and sync/ })).toHaveCount(0);
  await page.goto("/#/settings/account");
  await expect(page.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
});

test("creates an account with an invite and an emailed code, then pauses and resumes sync", async ({ page }) => {
  await open(page, "account", "#/settings/account");
  await expect(page.getByText("Use an account to sync your settings")).toBeVisible();
  await page.getByRole("button", { name: "Create account" }).click();
  await page.getByLabel("Invite code").fill("k7qm 2xwh 9qxa 3c2d");
  await expect(page.getByLabel("Invite code")).toHaveValue("K7QM-2XWH-9QXA-3C2D");
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Invite accepted")).toBeVisible();
  await page.getByLabel("Email").fill("new@example.com");
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Continue with a password" }).click();
  await page.getByLabel("8-character code").fill("abcdefgh");
  await page.getByRole("button", { name: "Verify" }).click();
  await expect(page.getByText("Your account is ready")).toBeVisible();
  await page.getByRole("button", { name: "Continue" }).click();
  await expect(page.getByText("Signed in on this device · This computer")).toBeVisible();

  const line = page.getByRole("button", { name: "Account and sync: Synced" });
  await expect(line).toBeVisible();
  await line.click();
  const popover = page.getByRole("dialog", { name: "Sync" });
  await expect(popover).toContainText("Up to date");
  await popover.getByRole("button", { name: "Pause sync" }).click();
  await expect(page.getByRole("button", { name: "Account and sync: Paused" })).toBeVisible();
  await popover.getByRole("button", { name: "Resume sync" }).click();
  await expect(page.getByRole("button", { name: "Account and sync: Synced" })).toBeVisible();
});

test("signs in with a wrong password, then right, and with a two-factor code", async ({ page }) => {
  await open(page, "account", "#/settings/account");
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.getByLabel("Email").fill("two@example.com");
  await page.getByLabel("Password").fill("nope");
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByText("That email and password don’t match. Check them and try again.")).toBeVisible();
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  await expect(page.getByText("Enter your two-factor code")).toBeVisible();
  await page.getByLabel("6-digit code").fill("000000");
  await page.getByRole("button", { name: "Verify" }).click();
  await expect(page.getByText(/4 tries left/)).toBeVisible();
  await page.getByLabel("6-digit code").fill("123456");
  await page.getByRole("button", { name: "Verify" }).click();
  await expect(page.getByText("Signed in on this device · This computer")).toBeVisible();
});

test("in a narrow window the sync status is one button in the title bar", async ({ page }) => {
  await page.setViewportSize({ width: 800, height: 820 });
  await open(page, "account", "#/settings/account");
  await page.getByRole("button", { name: "Sign in" }).click();
  await page.getByLabel("Email").fill("dev@example.com");
  await page.getByLabel("Password").fill(PASSWORD);
  await page.getByRole("button", { name: "Sign in" }).click();
  const button = page.locator(".titlebar").getByRole("button", { name: "Sync: Synced" });
  await expect(button).toBeVisible();
  await button.click();
  await expect(page.getByRole("dialog", { name: "Sync" })).toContainText("Your settings are the same on every signed-in computer.");
});
