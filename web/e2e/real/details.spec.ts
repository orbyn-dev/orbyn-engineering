import { expect, test, type Page } from "@playwright/test";
import { openDetails, openPanelTab, send, settled, startProject, threadIdOf, type Service } from "./service";

/** The thread's surroundings: details rows, the linked family (chip, panel tab, phone sheet), new messages, the Working shelf. */

interface Listed {
  id: string;
  title: string;
  lifecycle: { relation: string | null; parent_thread_id: string | null; visited_at?: string };
}

interface Family {
  thread_id: string;
  root_thread_id: string;
  threads: Listed[];
}

let service: Service;
let projectId = "";


const box = (page: Page) => page.getByRole("textbox", { name: /^Message/ });
const details = (page: Page) => page.getByRole("dialog", { name: "Thread details" });
const row = (page: Page, id: string) => page.locator(`li[data-thread-item="${id}"]`);
const panel = (page: Page) => page.getByRole("complementary", { name: "Thread context" });
const linkedTab = (page: Page) => panel(page).getByRole("tab", { name: "Linked threads", exact: true });
const fresh = async (page: Page, prompt: string) => {
  await page.goto(`${service.page}#/new/${projectId}`);
  await send(page, prompt);
  await expect(page).toHaveURL(/#\/threads\//);
  return threadIdOf(page);
};

const RIGHT_TABS = ["diff", "browser", "activity", "side", "tools", "linked", "review", "devices", "library"];

/** Saves the side panel open on Linked threads for one thread, as a person who left the window there finds it. */
async function savedOnLinkedThreads(page: Page, threadId: string) {
  const layout = { panelTab: "linked", panelTabOrder: RIGHT_TABS, panelTabsClosed: [] as string[], terminalOpen: false, panelMaximized: false, panelWidth: null, sideChat: null };
  await page.addInitScript(
    ({ id, saved }) => {
      localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelOpen: true, panelTab: "linked", panelTabOrder: saved.panelTabOrder, panelTabsClosed: [], layouts: { [id]: saved } }));
    },
    { id: threadId, saved: layout },
  );
}

/** Records the tab's loading bars: the family read can finish inside one frame, so the DOM is watched as it changes. */
async function watchLinkedLoading(page: Page) {
  await page.addInitScript(() => {
    const seen = { loading: false, bars: 0 };
    (window as unknown as { linkedLoadingSeen: typeof seen }).linkedLoadingSeen = seen;
    new MutationObserver(() => {
      const loading = document.querySelector('[aria-label="Loading linked threads"]');
      if (!loading) return;
      seen.loading = true;
      seen.bars = Math.max(seen.bars, loading.querySelectorAll(".fam-skel-row").length);
    }).observe(document, { subtree: true, childList: true });
  });
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ scenes: true, forge: true }));

});
test.afterAll(() => service.stop());
test.beforeEach(({ page }) => page.setViewportSize({ width: 1500, height: 900 }));

test("details shows linked pull requests, and right-click turns rows off and back", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] hello");
  await settled(service, id);
  await service.api("POST", `/api/threads/${id}/pulls`, { number: 42 });
  await page.reload();
  await openDetails(page);
  const pulls = details(page).getByRole("button", { name: "Pull requests" });
  await expect(pulls).toContainText("#42");
  await pulls.click();
  await expect(details(page).getByRole("button", { name: /#42 Fix the login/ })).toBeVisible();
  await details(page).click({ button: "right" });
  await page.getByRole("menuitemcheckbox", { name: "Pull requests" }).click();
  // Each toggle applies at once and the menu stays open.
  await expect(pulls).toBeHidden();
  await expect(page.getByRole("menu", { name: "Thread details rows" })).toBeVisible();
  await page.keyboard.press("Escape");
  await details(page).click({ button: "right" });
  await page.getByRole("menuitemcheckbox", { name: "Usage" }).click();
  await page.keyboard.press("Escape");
  await expect(details(page).getByText("Usage", { exact: true })).toBeVisible();
  await details(page).click({ button: "right" });
  await page.getByRole("menuitem", { name: "Reset to default" }).click();
  await expect(details(page).getByRole("button", { name: "Pull requests" })).toContainText("#42");
  await page.goto(`${service.page}#/settings/general`);
  // The count moves with the detail rows that exist, so pin its shape, not today's numbers.
  await expect(page.getByRole("button", { name: /\d+ of \d+ on/ })).toBeVisible();
});

test("Linked threads shows the family, and the fork says what it is a fork of", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] root");
  await settled(service, id);
  const fork = await service.api<Listed>("POST", `/api/threads/${id}/fork`, { title: "The fork" });
  await page.reload();
  await page.getByRole("button", { name: "Linked threads" }).click();
  const family = page.getByRole("dialog", { name: "Linked threads" });
  await expect(family).toBeVisible();
  await family.getByRole("link", { name: /The fork/ }).click();
  await expect(page).toHaveURL(new RegExp(fork.id));
  await expect(page.locator(".fam-line")).toContainText("Fork of");
  await page.getByRole("button", { name: "Linked threads" }).click();
  await expect(page.getByRole("dialog", { name: "Linked threads" }).getByRole("link", { name: /Root/ })).toBeVisible();
});

test("the Linked threads panel tab draws its loading bars before the family arrives", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] root");
  await settled(service, id);
  await service.api("POST", `/api/threads/${id}/fork`, { title: "The panel fork" });
  await watchLinkedLoading(page);
  await savedOnLinkedThreads(page, id);
  await page.reload();
  await expect(linkedTab(page)).toHaveAttribute("aria-selected", "true");
  const rows = panel(page).getByRole("link");
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText("Root");
  await expect(rows.nth(1)).toContainText("Fork");
  await expect(rows.nth(1)).toContainText("The panel fork");
  // The tab's three quiet bars were in the DOM while the real family read was in flight; nothing slowed or stubbed it.
  const seen = await page.evaluate(() => (window as unknown as { linkedLoadingSeen: { loading: boolean; bars: number } }).linkedLoadingSeen);
  expect(seen).toEqual({ loading: true, bars: 3 });
});

test("the Linked threads panel tab says when there are none and starts a side chat", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] root");
  await settled(service, id);
  const sidePanel = await openPanelTab(page, "Linked threads");
  await expect(sidePanel.getByText("No linked threads")).toBeVisible();
  await expect(sidePanel.getByText("Fork this thread to try another approach, or start a linked thread to ask something on the side.")).toBeVisible();
  const created = page.waitForResponse(
    (response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${id}/fork`,
  );
  await sidePanel.getByRole("button", { name: "Start a linked thread" }).click();
  const answer = await created;
  expect(answer.status()).toBe(201);
  const side = (await answer.json()) as Listed;
  expect(side.title).toBe("Side chat");
  expect(side.lifecycle).toMatchObject({ relation: "side", parent_thread_id: id });
  // The service's own family read holds the side chat, and the tab shows the relation once it reads the family again.
  const family = await service.api<Family>("GET", `/api/threads/${id}/family`);
  expect(family.root_thread_id).toBe(id);
  expect(family.threads.map((thread) => thread.id).sort()).toEqual([id, side.id].sort());
  expect(family.threads.find((thread) => thread.id === side.id)?.lifecycle).toMatchObject({ relation: "side", parent_thread_id: id });
  await expect(sidePanel.getByRole("tab", { name: "Side chat" })).toHaveAttribute("aria-selected", "true");
  await sidePanel.getByRole("tab", { name: "Linked threads" }).click();
  const rows = sidePanel.getByRole("link");
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText("Root");
  await expect(rows.first()).toContainText("This thread");
  await expect(rows.nth(1)).toContainText("Side chat");
  await expect(page.getByRole("button", { name: "Linked threads", exact: true })).toHaveText(/Linked 1/);
});

test("the Linked chip opens the family as a sheet on a phone", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] root");
  await settled(service, id);
  await service.api("POST", `/api/threads/${id}/fork`, { title: "The phone fork" });
  await page.setViewportSize({ width: 390, height: 844 });
  await page.reload();
  const chip = page.getByRole("button", { name: "Linked threads", exact: true });
  await expect(chip).toContainText("Linked 1");
  await chip.click();
  const sheet = page.getByRole("dialog", { name: "Linked threads" });
  await expect(sheet).toBeVisible();
  await expect(sheet).toHaveAttribute("aria-modal", "true");
  await expect(sheet.locator("[data-sheet-handle]")).toBeVisible();
  const rows = sheet.getByRole("link");
  await expect(rows).toHaveCount(2);
  await expect(rows.first()).toContainText("Root");
  await expect(rows.nth(1)).toContainText("Fork");
  await expect(rows.nth(1)).toContainText("The phone fork");
  await sheet.getByRole("button", { name: "Close" }).click();
  await expect(sheet).toBeHidden();
});

test("a thread you left shows where the new messages start", async ({ page }) => {
  const id = await fresh(page, "one");
  await settled(service, id);
  await expect(page.getByText("hello from codex")).toBeVisible();
  await page.getByRole("link", { name: "New thread", exact: true }).click();
  await expect(page).not.toHaveURL(/#\/threads\//);
  await page.waitForTimeout(1100);
  await service.api("POST", `/api/threads/${id}/runs`, { prompt: "two" });
  await settled(service, id);
  await page.goto(`${service.page}#/threads/${id}`);
  await expect(page.getByText("New since you were here")).toBeVisible();
  // Your own message in the same visit doesn't move it, and the next visit clears it.
  await send(page, "three");
  await settled(service, id);
  await expect(page.getByText("New since you were here")).toHaveCount(1);
  // The mark after the reply has gone, so the next one is the mark for leaving.
  await page.waitForTimeout(800);
  const left = page.waitForResponse((response) => response.request().method() === "PATCH");
  await page.getByRole("link", { name: "New thread", exact: true }).click();
  await left;
  await page.goto(`${service.page}#/threads/${id}`);
  await expect(box(page)).toBeVisible();
  await expect(page.getByText("New since you were here")).toHaveCount(0);
});

test("a watched thread goes to the Working shelf, unless that is off", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] watch");
  await settled(service, id);
  await service.api("POST", `/api/threads/${id}/pulls`, { number: 77 });
  await service.api("POST", `/api/threads/${id}/pulls/77/watch`, {});
  await page.goto(`${service.page}#/new/${projectId}`);
  await expect(row(page, id)).toHaveAttribute("data-section", "working");
  await page.goto(`${service.page}#/settings/general`);
  await page.getByRole("region", { name: "Settings" }).getByRole("switch", { name: "Working section" }).click();
  await page.goto(`${service.page}#/new/${projectId}`);
  await expect(row(page, id)).toHaveAttribute("data-section", "active");
});

test("user messages keep same-thread editing and assistant responses keep forks", async ({ page }) => {
  const id = await fresh(page, "[scene:usage] first");
  await settled(service, id);
  await page.locator(".msg-user").last().hover();
  await expect(page.getByRole("button", { name: "Edit in a new thread" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Edit from here" }).last()).toBeEnabled();
  await expect(page.getByRole("button", { name: "Fork from this response" }).last()).toBeEnabled();
  await page.getByRole("button", { name: "Edit from here" }).last().click();
  await expect(page.getByRole("dialog", { name: "Edit from here?" })).toBeVisible();
  await page.getByRole("dialog", { name: "Edit from here?" }).getByRole("button", { name: "Cancel", exact: true }).click();
  await expect(page).toHaveURL(new RegExp(id));
  await expect(page.locator(".msg-user")).toHaveCount(1);
});

test("a pull request link opens its page, previews on hover, and its menu offers the host", async ({ page }) => {
  const id = await fresh(page, "[scene:pull-link] go");
  await settled(service, id);
  const link = page.getByRole("link", { name: "the pull request" });
  await link.hover();
  const preview = page.locator("[data-pull-preview]");
  await expect(preview).toBeVisible();
  await expect(preview).toContainText("Fix the login");
  await expect(preview).toContainText("o/r#42 · Open");
  await link.click();
  await expect(preview).toHaveCount(0);
  await expect(page).toHaveURL(/#\/pulls\/[^/]+\/42$/);
  await page.goBack();
  await link.click({ button: "right" });
  await expect(page.getByRole("menuitem", { name: "Open on the host" })).toBeVisible();
  await expect(page.getByRole("menuitem", { name: "Copy link" })).toBeVisible();
});
