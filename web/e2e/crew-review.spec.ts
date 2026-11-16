import { expect, test, type Page } from "@playwright/test";

/** The Autopilot review frames of Crew Designs v19 in the dev build, where the Preview screens exist; they make no API request. */
const open = (page: Page, frame: string) => page.goto(`/#/preview/crew/${frame}`);

test("the Review queue ranks by risk, counts add up and filters by keyboard", async ({ page }) => {
  await open(page, "review-queue");
  await expect(page.getByRole("tab", { name: "Review · 7" })).toBeVisible();
  const cards = page.getByRole("list", { name: "Merges to review" }).getByRole("listitem");
  await expect(cards).toHaveCount(4);
  await expect(cards.first()).toContainText("Rotate the signing key");
  await expect(cards.first()).toContainText("High");

  await page.getByRole("button", { name: "High 1" }).focus();
  await page.keyboard.press("Space");
  await expect(cards).toHaveCount(1);
  await expect(page.getByRole("button", { name: "Overdue 0" })).toBeDisabled();
  await page.getByRole("button", { name: "To review 7" }).click();
  await expect(cards).toHaveCount(4);
});

test("a merge opens from its title, Revert asks first, and nothing can be confirmed without the backend", async ({ page }) => {
  await open(page, "review-queue");
  await page.getByRole("button", { name: "Use next in the auth redirect" }).click();
  await expect(page).toHaveURL(/review-item$/);
  await expect(page.getByRole("region", { name: "Not verified" })).toContainText("Safari and Firefox behaviour");

  await page.getByRole("button", { name: "Revert", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Revert this merge" });
  await expect(dialog).toBeVisible();
  await expect(dialog).toContainText("Main isn’t changed until the revert passes its checks");
  const confirm = dialog.getByRole("button", { name: "Open the revert" });
  await expect(confirm).toBeDisabled();
  await expect(confirm).toHaveAttribute("title", "Crew is not connected to this app yet");

  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(page.getByRole("button", { name: "Revert", exact: true })).toBeFocused();
});

test("Comment opens a fix brief dialog that traps focus and closes with Cancel", async ({ page }) => {
  await open(page, "review-item");
  await page.getByRole("button", { name: "Comment" }).click();
  const dialog = page.getByRole("dialog", { name: "Comment on this merge" });
  await expect(dialog.getByLabel("What should change?")).toBeFocused();
  await expect(dialog.getByRole("button", { name: "Send as a fix brief" })).toBeDisabled();
  await dialog.getByRole("button", { name: "Cancel" }).click();
  await expect(dialog).toBeHidden();
});

test("later merges on top: the revert dialog lists them and offers one alone or both", async ({ page }) => {
  await open(page, "review-revert-dependents");
  const dialog = page.getByRole("dialog", { name: "Another merge depends on this one" });
  await expect(dialog).toContainText("f1a0c63 · Add a rate limit to the login route");
  await expect(dialog.getByRole("button", { name: "Revert this one alone" })).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "Revert both together" })).toBeDisabled();
  await expect(dialog.getByRole("button", { name: "Cancel" })).toBeFocused();
});

test("the tool tabs go Review queue, Merge queue, Digest, and back to an empty queue state", async ({ page }) => {
  await open(page, "review-queue");
  await page.getByRole("tab", { name: "Merge queue · 3" }).click();
  await expect(page.getByText("Re-checking combined").first()).toBeVisible();
  await page.getByRole("tab", { name: "Digest" }).click();
  await expect(page.getByText("Waiting for your review")).toBeVisible();
  await open(page, "review-empty");
  await expect(page.getByText("Nothing to review")).toBeVisible();
  await page.getByRole("button", { name: "Open the daily digest" }).click();
  await expect(page).toHaveURL(/digest-daily$/);
});

test("a batch counts what is ticked", async ({ page }) => {
  await open(page, "review-batch");
  await page.getByRole("checkbox", { name: /Update the login docs/ }).uncheck({ force: true });
  await expect(page.getByRole("button", { name: "Looks good for 11" })).toBeDisabled();
  await page.getByRole("button", { name: "Untick all" }).click();
  await expect(page.getByRole("button", { name: "Looks good for 6" })).toBeVisible();
});

test("a narrow window stacks each card in one column with the actions on the card", async ({ page }) => {
  await page.setViewportSize({ width: 640, height: 900 });
  await open(page, "review-narrow-window");
  const card = page.getByRole("article", { name: /Rotate the signing key/ });
  const title = await card.getByRole("button", { name: "Rotate the signing key" }).boundingBox();
  const actions = await card.getByRole("button", { name: "Looks good" }).boundingBox();
  expect(title && actions && actions.y > title.y).toBe(true);
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test("the side panel switches between Review and Merge queue", async ({ page }) => {
  await open(page, "review-narrow-panel");
  await expect(page.getByText("top of queue")).toBeVisible();
  await page.getByRole("tab", { name: "Merge queue" }).click();
  await expect(page.getByText("Re-checking combined")).toBeVisible();
});

test("watch on main: a failed main is red, and an unsure Crew asks with its options", async ({ page }) => {
  await open(page, "watch-asks");
  await expect(page.getByRole("alert").first()).toContainText("Crew isn’t certain why");
  await expect(page.getByRole("button", { name: "Revert both" })).toBeDisabled();
  await open(page, "watch-reverted");
  await expect(page.getByText("Main is healthy again at 15:34.")).toBeVisible();
});
