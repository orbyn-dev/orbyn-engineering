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
  const drawer = page.getByRole("complementary", { name: "Install zod@3.24.1" });
  await expect(drawer).toBeFocused();
  await expect(drawer).toContainText("Second key: yes");
  await page.keyboard.press("Escape");
  await expect(drawer).toHaveCount(0);
  // Another request, by keyboard: Enter on the focused row button opens it.
  await table.getByRole("button", { name: /Publish the package/ }).focus();
  await page.keyboard.press("Enter");
  const person = page.getByRole("complementary", { name: "Publish the package to the registry" });
  await expect(person).toContainText("not blocking anything");
  // Answering is not connected yet: the control says so instead of doing nothing.
  await expect(person.getByRole("button", { name: "Dismiss" })).toBeDisabled();
});

test("moves between the Crew tabs with the arrow keys", async ({ page }) => {
  await open(page, "crew", "#/crew/approvals");
  await page.getByRole("tab", { name: "Approvals" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page).toHaveURL(/#\/crew\/trust$/);
  await expect(page.getByRole("table", { name: "By engine" })).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(page).toHaveURL(/#\/crew\/background$/);
  await expect(page.getByText("No background crews yet")).toBeVisible();
});

test("a long log filters the loaded requests and reads the next page on scroll", async ({ page }) => {
  await open(page, "crew-many", "#/crew/approvals");
  await expect(page.getByRole("status").filter({ hasText: "Showing" })).toContainText("of the 18 loaded requests");
  await page.getByRole("button", { name: /^Denied by rule/ }).click();
  await expect(page.getByRole("table", { name: "Approval log" }).getByRole("row").nth(1)).toContainText("Denied by rule");
  await page.getByRole("button", { name: /^All \d/ }).click();
  await page.getByRole("region", { name: "Requests" }).evaluate((el) => { el.scrollTop = el.scrollHeight; });
  await expect(page.getByRole("status").filter({ hasText: "Showing" })).toContainText("of the 36 loaded requests");
});

test("a task stopped by a trip wire is amber and says nothing was lost", async ({ page }) => {
  await open(page, "crew-tripwire", "#/crew/approvals");
  const stop = page.getByRole("region", { name: "Stopped: task-client" });
  await expect(stop).toContainText("Nothing was lost; its worktree is intact.");
  await expect(stop.getByRole("table", { name: "Denied requests" }).getByRole("row")).toHaveCount(4);
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

test("background crews say Crew is not connected yet and offer nothing that does nothing", async ({ page }) => {
  await open(page, "crew", "#/crew/background");
  await expect(page.getByText("No background crews yet")).toBeVisible();
  await expect(page.getByRole("button", { name: "New background crew" })).toBeDisabled();
  await expect(page.getByRole("button", { name: "Use the Flaky test template" })).toBeDisabled();
  await expect(page.getByRole("table", { name: "Triggers" })).toBeVisible();
});
