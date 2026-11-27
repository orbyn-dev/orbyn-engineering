import { expect, test, type Page } from "@playwright/test";
import { openDetails, send, settled, startProject, threadIdOf, type Service } from "./service";

// Each test makes its own thread and its own links, so any test runs alone and a retry starts clean; serial order only keeps
// one service at a time.
test.describe.configure({ mode: "serial" });

/**
 * F69: thread issue links, through a real `orbynd` with `ORBYN_ISSUES=1` and a stand-in `gh` holding the issues. No real
 * GitHub is contacted: the service's GitHub reader runs the stand-in, and the connection names the `o/r` scope it answers for.
 *
 * Covered: linking from the Details card's Link an issue search over the real `find` route, the header issue chip and the
 * thread list marker, the issue page with its Linked threads card, unlinking from the Details row, the chip's own link form
 * with its not-found error and its successful second link, and the chip as a bottom sheet on a phone whose long press arms
 * the row and unlinks from it.
 *
 * Not covered, and why: a failing tracker is not named under the search because `GET /api/issues/find` answers a plain
 * `IssueRef[]` with no per-tracker failure, and the four token trackers that could fail need a keychain token (an owner
 * action); the Linked threads card's read-failure notice and Retry need `GET /api/issues/links` to fail, but that is the
 * service's own store read and no real-service route can make it fail without a page mock; a real tracker is an owner action.
 */

test.setTimeout(60_000);

const WINDOW = { width: 1500, height: 1000 };

let service: Service;
let projectId = "";
let threadId = "";
let threadTitle = "";

interface IssueRef {
  tracker: string;
  scope: string;
  key: string;
  display_key: string;
  title: string;
  state: { category: "open" | "closed"; name: string; close_reason?: string | null };
  labels: { name: string }[];
}

const links = () => service.api<Record<string, IssueRef[]>>("GET", "/api/issues/links");
const chip = (page: Page) => page.locator(".iss-chip-btn");

/**
 * The thread's title once its first message has named it. The title run is detached from the
 * turn, so the API still says "New thread" for a moment after the turn settles; assert the name
 * it settles on rather than race that run (which a later read would then make transient).
 */
async function named(id: string) {
  let title = "";
  await expect
    .poll(
      async () => {
        title = (await service.api<{ title: string }>("GET", `/api/threads/${id}`)).title;
        return title;
      },
      { timeout: 15_000, message: "the first message names the thread" },
    )
    .not.toBe("New thread");
  return title;
}

/** Opens the thread page the fixture started, and the Details popover. */
async function threadWithDetails(page: Page) {
  await page.goto(`${service.page}#/threads/${threadId}`);
  return openDetails(page);
}

/** Links one issue through the Details card's dialog, the search over the real `find` route. */
async function linkFromDetails(page: Page, search: string, option: RegExp) {
  const card = await threadWithDetails(page);
  await card.getByRole("button", { name: "Link an issue…" }).click();
  const dialog = page.getByRole("dialog", { name: "Link an issue" });
  await expect(dialog).toBeVisible();
  await dialog.getByLabel("Search issues").fill(search);
  await dialog.getByRole("option", { name: option }).click();
  await dialog.getByRole("button", { name: "Link", exact: true }).click();
  await expect(dialog).toBeHidden();
  return card;
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ forge: true, issues: true, fakeEngines: true }));
  // The real connect route stores the scope the stand-in `gh` answers for; no token crosses (GitHub uses gh's own sign-in).
  const row = await service.api<{ tracker: string; connected: boolean; scope: string | null }>("POST", "/api/issues/trackers/github/connect", { scope: "o/r" });
  expect(row).toMatchObject({ tracker: "github", connected: true, scope: "o/r" });
});
test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await page.setViewportSize(WINDOW);
  threadId = "";
  threadTitle = "";
  // Every test starts a thread of its own: nothing waits for state an earlier test left (a title filter or a retry is sound).
  await page.goto(`${service.page}#/new/${projectId}`);
  await send(page, "start on the retry issue");
  await expect(page).toHaveURL(/#\/threads\//);
  threadId = threadIdOf(page);
  await settled(service, threadId);
  threadTitle = await named(threadId);
});
test.afterEach(async () => {
  // Leave no link behind: the issue page's Linked threads card is checked per test, so a retry starts from the same state.
  const byThread = await service.api<Record<string, IssueRef[]>>("GET", "/api/issues/links");
  for (const ref of byThread[threadId] ?? []) {
    await service.api("POST", `/api/threads/${threadId}/issue-links/remove`, { tracker: ref.tracker, scope: ref.scope, key: ref.key });
  }
});

test("the Details card links an issue from its search dialog, and the chip and thread row mark it", async ({ page }) => {
  const card = await threadWithDetails(page);
  // With no tracker row yet the row is not drawn; the issue row says none is linked and offers the dialog.
  await expect(card.getByText("None linked")).toBeVisible();
  await card.getByRole("button", { name: "Link an issue…" }).click();
  const dialog = page.getByRole("dialog", { name: "Link an issue" });
  await expect(dialog).toBeVisible();
  // The dialog searches the connected tracker through GET /api/issues/find; a number is looked up as the issue key.
  await dialog.getByLabel("Search issues").fill("7");
  const option = dialog.getByRole("option", { name: /#7 Retry the queue/ });
  await expect(option).toBeVisible();
  await expect(option).toContainText("GitHub · o/r");
  await option.click();
  const written = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${threadId}/issue-links`);
  await dialog.getByRole("button", { name: "Link", exact: true }).click();
  const answer = await written;
  expect(answer.status()).toBe(204);
  await expect(dialog).toBeHidden();

  // The service keeps the link and its IssueRef snapshot beside the thread.
  const byThread = await links();
  expect(byThread[threadId]).toHaveLength(1);
  expect(byThread[threadId]![0]).toMatchObject({ tracker: "github", scope: "o/r", key: "7", display_key: "#7", title: "Retry the queue", state: { category: "open" } });

  // The Details card draws the row: GitHub · #7, its state as a shape and a word, the title, and Unlink on the line.
  const row = card.getByRole("button", { name: "#7 Retry the queue" });
  await expect(row).toBeVisible();
  await expect(row).toContainText("GitHub · #7");
  await expect(row).toContainText("Retry the queue");
  await expect(card.getByRole("button", { name: "Unlink #7" })).toBeVisible();

  // The header chip left of the pull request chip, and the thread list row's marker.
  await expect(chip(page)).toHaveAttribute("aria-label", "Issues");
  await expect(chip(page)).toContainText("#7");
  await expect(page.getByRole("img", { name: "Issue #7, open" })).toBeVisible();

  // A press on the row opens the issue beside the thread, in the side panel.
  await row.click();
  const panel = page.getByRole("complementary", { name: "Thread context" });
  await expect(panel).toBeVisible();
  await expect(panel.locator(".iss-panel")).toContainText("Retry the queue");
  await expect(panel.locator(".iss-panel")).toContainText("The queue stalls when the worker restarts.");
});

test("the issue page shows the Linked threads card, and Unlink removes this thread's link only", async ({ page }) => {
  // This test links the issue itself; nothing is inherited from the first test.
  await linkFromDetails(page, "7", /#7 Retry the queue/);
  await page.goto(`${service.page}#/issues/github/o%2Fr/7`);
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible();
  await expect(page.locator(".page-sub")).toContainText("#7 · GitHub · o/r");
  // The issue as the tracker reports it: body, comment, facts, labels and assignees.
  await expect(page.locator(".iss-body").first()).toContainText("The queue stalls when the worker restarts.");
  await expect(page.getByText("I can reproduce it.")).toBeVisible();
  const facts = page.getByRole("group", { name: "Facts" });
  await expect(facts).toContainText("Tracker");
  await expect(facts).toContainText("GitHub");
  await expect(facts).toContainText("grace");
  await expect(facts).toContainText("bug");
  // The Linked threads card: the thread that links this issue, from GET /api/issues/links.
  const card = page.getByRole("group", { name: "Linked threads" });
  await expect(card).toContainText(threadTitle);
  await expect(card.locator(`a[href="#/threads/${threadId}"]`)).toBeVisible();
  await expect(card.getByRole("button", { name: "Start a thread from this issue" })).toBeVisible();

  // Unlink from the Details row: one write, this thread's link only.
  const row = await threadWithDetails(page);
  const removed = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${threadId}/issue-links/remove`);
  await row.getByRole("button", { name: "Unlink #7" }).click();
  expect((await removed).status()).toBe(204);
  await expect(row.getByText("None linked")).toBeVisible();
  expect((await links())[threadId] ?? []).toEqual([]);
  await expect(chip(page)).toHaveCount(0);

  // The issue itself is untouched; only the link is gone.
  await page.goto(`${service.page}#/issues/github/o%2Fr/7`);
  await expect(page.getByRole("group", { name: "Linked threads" })).toContainText("No thread is linked to this issue yet.");
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible();
});

test("the chip links and unlinks with its own form, refuses an unknown issue, and is a sheet on a phone", async ({ page }) => {
  await linkFromDetails(page, "12", /#12 Tidy the docs/);
  await expect(chip(page)).toContainText("#12");

  // The chip opens its list; its Link an issue… form searches as you type and says what it could not find.
  await chip(page).click();
  const list = page.getByRole("dialog", { name: "Issues" });
  await expect(list).toContainText("1 linked issue");
  await expect(list.getByRole("button", { name: "#12 Tidy the docs" })).toBeVisible();
  await list.getByRole("button", { name: "Link an issue…" }).click();
  const form = list.getByRole("combobox", { name: "Issue number or link" });
  await form.fill("999");
  await list.getByRole("button", { name: "Link", exact: true }).click();
  await expect(list.getByRole("alert")).toHaveText("No issue #999 in o/r on GitHub.");
  // Nothing was written for the unknown issue.
  expect((await links())[threadId]!.map((ref) => ref.key)).toEqual(["12"]);
  await list.getByRole("button", { name: "Cancel" }).click();
  await expect(list).toContainText("1 linked issue");

  // The chip's own Unlink removes the link from this thread only.
  const removed = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${threadId}/issue-links/remove`);
  await list.getByRole("button", { name: "Unlink #12" }).click();
  expect((await removed).status()).toBe(204);
  await expect(chip(page)).toHaveCount(0);
  expect((await links())[threadId] ?? []).toEqual([]);

  // On a phone-width window the chip opens a dialog with the same list and a Close button (that it is drawn as a sheet is not checked).
  await linkFromDetails(page, "7", /#7 Retry the queue/);
  await page.setViewportSize({ width: 420, height: 860 });
  await chip(page).click();
  const sheet = page.getByRole("dialog", { name: "Issues" });
  await expect(sheet).toBeVisible();
  await expect(sheet).toContainText("1 linked issue");
  await expect(sheet.getByRole("button", { name: "#7 Retry the queue" })).toBeVisible();
  await expect(sheet.getByRole("button", { name: "Close" })).toBeVisible();

  // A held press is what arms a row on a phone: down, hold past 500 ms, then off the row before letting go (a release on
  // the row is its open click, which navigates to the issue). Playwright's mouse also hovers, so the hold is proven by the
  // row's own data-armed attribute, not by hover; the Unlink is then one route write.
  const open = sheet.getByRole("button", { name: "#7 Retry the queue" });
  // The row's text is split across its key and title spans (#7 + Retry the queue, no space between), so match the title.
  const prow = sheet.locator(".iss-prow").filter({ hasText: "Retry the queue" });
  const unlink = sheet.getByRole("button", { name: "Unlink #7" });
  await expect(prow).not.toHaveAttribute("data-armed", "true");
  // hover() waits for the row to be stable (the sheet is still rising), so the raw mouse press cannot land off it.
  await open.hover();
  const at = await open.boundingBox();
  const head = await sheet.getByText("1 linked issue").boundingBox();
  if (at === null || head === null) throw new Error("the phone sheet did not lay its row out");
  await page.mouse.move(at.x + at.width / 2, at.y + at.height / 2);
  await page.mouse.down();
  await expect(prow).toHaveAttribute("data-armed", "true");
  await page.mouse.move(head.x + head.width / 2, head.y + head.height / 2);
  await expect(unlink).toHaveCSS("opacity", "1");
  await page.mouse.up();
  // The release landed off the row, so the open click did not run: still on the thread, and the row is still armed.
  expect(new URL(page.url()).hash).toBe(`#/threads/${threadId}`);
  await expect(unlink).toHaveCSS("opacity", "1");
  let unlinks = 0;
  page.on("request", (request) => { if (request.method() === "POST" && new URL(request.url()).pathname === `/api/threads/${threadId}/issue-links/remove`) unlinks += 1; });
  const unlinked = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${threadId}/issue-links/remove`);
  await unlink.click();
  expect((await unlinked).status()).toBe(204);
  expect(unlinks).toBe(1);
  await expect(chip(page)).toHaveCount(0);
  expect((await links())[threadId] ?? []).toEqual([]);
});

test("the chip's own form links a second issue through the real find route", async ({ page }) => {
  await linkFromDetails(page, "7", /#7 Retry the queue/);
  await expect(chip(page)).toContainText("#7");
  await chip(page).click();
  const list = page.getByRole("dialog", { name: "Issues" });
  await expect(list).toContainText("1 linked issue");
  await list.getByRole("button", { name: "Link an issue…" }).click();
  const form = list.getByRole("combobox", { name: "Issue number or link" });
  // The chip's form searches the real find route as the person types; a number is looked up as the issue key.
  const searched = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/issues/find");
  await form.fill("12");
  const found = await searched;
  expect(found.status()).toBe(200);
  expect((await found.json()) as IssueRef[]).toEqual([
    {
      tracker: "github",
      scope: "o/r",
      key: "12",
      display_key: "#12",
      title: "Tidy the docs",
      state: { category: "open", name: "Open", close_reason: null },
      labels: [{ name: "docs" }],
    },
  ]);
  const option = list.getByRole("option", { name: /#12 Tidy the docs/ });
  await expect(option).toBeVisible();
  await option.click();
  // Choosing writes nothing; Link is the one write, and the service keeps the link with its issue snapshot.
  await expect(list.getByRole("button", { name: "Link", exact: true })).toBeEnabled();
  expect((await links())[threadId] ?? []).toHaveLength(1);
  let linkPosts = 0;
  page.on("request", (request) => { if (request.method() === "POST" && new URL(request.url()).pathname === `/api/threads/${threadId}/issue-links`) linkPosts += 1; });
  const written = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === `/api/threads/${threadId}/issue-links`);
  await list.getByRole("button", { name: "Link", exact: true }).click();
  expect((await written).status()).toBe(204);
  expect(linkPosts).toBe(1);
  expect((await links())[threadId]!.map((ref) => ref.key)).toEqual(["7", "12"]);
  // The chip reloads the thread's links: two now, both named in the open list.
  await expect(chip(page)).toContainText("Issues 2");
  await expect(list).toContainText("2 linked issues");
  await expect(list.getByRole("button", { name: "#7 Retry the queue" })).toBeVisible();
  await expect(list.getByRole("button", { name: "#12 Tidy the docs" })).toBeVisible();
});
