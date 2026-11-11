import { expect, test, type Page } from "@playwright/test";

/** Crew Preview against the mock: `crew`, `crew-<state>` turn the flag on; the default scenario leaves it off. */
async function open(page: Page, scenario: string, hash: string) {
  await page.goto(`/__mock/scenario/${scenario}`);
  await page.goto(`/${hash}`);
}

test("with Crew Preview off nothing of Crew is drawn", async ({ page }) => {
  await open(page, "default", "#/crew/approvals");
  await expect(page.getByRole("tablist", { name: "Crew" })).toHaveCount(0);
  await page.goto("/#/settings/crew");
  await expect(page.getByRole("link", { name: "General" })).toHaveAttribute("aria-current", "page");
  await expect(page.getByRole("link", { name: "Crew", exact: true })).toHaveCount(0);
});

test("reads the approval log, opens one request's chain and closes it with Escape", async ({ page }) => {
  await open(page, "crew", "#/crew/approvals");
  const table = page.getByRole("table", { name: "Approval log" });
  await expect(table.getByRole("row")).toHaveCount(7);
  await table.getByRole("button", { name: /Install zod/ }).click();
  const drawer = page.getByRole("complementary", { name: "Install zod 3.24 as a dev dependency" });
  await expect(drawer).toBeFocused();
  await expect(drawer).toContainText("Second key: yes");
  await page.keyboard.press("Escape");
  await expect(drawer).toHaveCount(0);
  // Another request, by keyboard: Enter on the focused row button opens it.
  await table.getByRole("button", { name: /Publish the package/ }).focus();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("complementary", { name: "Publish the package to the registry" })).toContainText("not blocking anything");
});

test("moves between the Crew tabs with the arrow keys", async ({ page }) => {
  await open(page, "crew", "#/crew/approvals");
  await page.getByRole("tab", { name: "Approvals" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page).toHaveURL(/#\/crew\/trust$/);
  await expect(page.getByRole("table", { name: "By engine" })).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(page).toHaveURL(/#\/crew\/background$/);
  await expect(page.getByRole("table", { name: "Background crews" })).toBeVisible();
});

test("a long log filters by result and keeps the real totals", async ({ page }) => {
  await open(page, "crew-many", "#/crew/approvals");
  await expect(page.getByRole("button", { name: /^All 214/ })).toBeVisible();
  await page.getByRole("button", { name: /^Denied by rule 14/ }).click();
  await expect(page.getByRole("table", { name: "Approval log" }).getByRole("row").nth(1)).toContainText("Denied by rule");
  await expect(page.getByRole("button", { name: /^All 214/ })).toBeVisible();
});

test("a guarded command held for you is answered on its own, once", async ({ page }) => {
  await open(page, "crew-guard-on", "#/crew/approvals");
  await page.getByRole("button", { name: /Delete test orders in prod-db-1/ }).click();
  const drawer = page.getByRole("complementary", { name: "Delete test orders in prod-db-1" });
  await expect(drawer).toContainText("Held for you");
  await drawer.getByRole("button", { name: "Approve once" }).click();
  await expect(page.getByRole("button", { name: /Delete test orders in prod-db-1/ })).toHaveCount(0);
});

test("approving a command yourself names it in a dialog that Cancel closes without approving", async ({ page }) => {
  await open(page, "crew-guard", "#/crew/approvals");
  await page.getByRole("button", { name: /Truncate audit_log/ }).click();
  await page.getByRole("button", { name: "Approve it myself" }).click();
  const dialog = page.getByRole("dialog", { name: "Approve this command yourself?" });
  await expect(dialog).toContainText('TRUNCATE audit_log');
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Truncate audit_log/ })).toBeVisible();
});

test("a task stopped by a trip wire is amber and says nothing was lost", async ({ page }) => {
  await open(page, "crew-tripwire", "#/crew/approvals");
  const stop = page.getByRole("region", { name: "Stopped: Regenerate the client" });
  await expect(stop).toContainText("Nothing was lost; its worktree is intact.");
  await stop.getByRole("button", { name: "Review the denials" }).click();
  await expect(stop.getByRole("table", { name: "Denied requests" })).toBeFocused();
});

test("changes Autopilot in Settings with the keyboard and the setting sticks", async ({ page }) => {
  await open(page, "crew", "#/settings/crew");
  const full = page.getByRole("radio", { name: /^Full\s*Default/ });
  await expect(full).toBeChecked();
  await full.focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("radio", { name: /^Off/ })).toBeChecked();
  await expect(page.getByText("Off: agents will ask you")).toBeVisible();
  await page.reload();
  await expect(page.getByRole("radio", { name: /^Off/ })).toBeChecked();
});

test("creates a background crew from a template and turns another off", async ({ page }) => {
  await open(page, "crew", "#/crew/background");
  await page.getByRole("button", { name: "Use the Flaky test template" }).click();
  const dialog = page.getByRole("dialog", { name: "Create a background crew" });
  await expect(dialog.getByRole("radio", { name: "Flaky test" })).toBeChecked();
  await dialog.getByRole("button", { name: "Create background crew" }).click();
  await expect(dialog).toHaveCount(0);
  await expect(page.getByRole("table", { name: "Background crews" }).getByRole("row")).toHaveCount(6);
  const sw = page.getByRole("switch", { name: "Fix failing CI state" });
  await sw.click();
  await expect(sw).not.toBeChecked();
});
