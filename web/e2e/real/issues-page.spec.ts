import { expect, test, type Page } from "@playwright/test";
import { settled, startProject, threadIdOf, type Service } from "./service";

/**
 * F68: the Issues page against a real `orbynd` with `ORBYN_ISSUES=1` and a stand-in `gh`. No real GitHub is contacted; the
 * connection names the `o/r` scope the stand-in answers for.
 *
 * Covered: the list with its tracker, counts, state tabs and the label, assignee and text filters, no-match and closed rows,
 * and the absence of a Load more row; the detail page with body, comments, facts and its linked pull requests card absent
 * (the fixture reports none); the comment box's one write; the description pencil's one update; the label picker's candidates
 * read and one update of the whole set, and an assignee the candidates cannot address; the State menu closing and reopening
 * the issue; the Sort menu's two orders; and Start a thread, the service's framed draft read and one write that creates,
 * links and queues. The issue-edit routes are served for GitHub (#1593), so the candidates probe answers 400 invalid (a live
 * route) and the edit and Sort controls are live; the stand-in `gh` answers the PATCH, comment POST and assignees read those
 * edits make, so they arrive at a tracker (Start a thread, the connect route and the project preferences PUT stay inside
 * the service).
 *
 * The listing is cached per connection scope and full query for 30 s, and a write does not clear it (crates/orbyn-core/src/
 * issues, `CACHE_FOR` and `scope_list`), so a query key an earlier test already read can answer with the pre-write order. The
 * Sort test therefore reads filter+sort keys first requested after its own write and asserts no order on the default list.
 *
 * Not covered, and why: the four other trackers keep update, state and candidates Planned (422 unsupported) and need a
 * keychain token to connect at all (an owner action); a "no such route" write (the routes exist on this build); paging (the
 * fixture issues no cursor); the empty, loading and error list states; and phone width.
 */

test.setTimeout(60_000);

const WINDOW = { width: 1500, height: 1000 };

let service: Service;
let projectId = "";

const rows = (page: Page) => page.locator(".iss-rows > li");
const search = (page: Page) => page.getByRole("search").getByRole("searchbox", { name: "Search issues" });

test.beforeAll(async () => {
  const started = await startProject({ forge: true, issues: true, fakeEngines: true });
  ({ service, projectId } = started);
  await service.api("POST", "/api/issues/trackers/github/connect", { scope: "o/r" });
  // Starting a thread from an issue reads the project's saved engine (New thread hands one in from its own pickers).
  await service.api("PUT", `/api/projects/${projectId}/preferences`, { values: { default_engine: started.engines[0]!.id } });
});
test.afterAll(() => service.stop());
test.beforeEach(async ({ page }) => {
  await page.setViewportSize(WINDOW);
});

test("the list shows the tracker's issues with counts, state tabs and the filters", async ({ page }) => {
  await page.goto(`${service.page}#/issues`);
  await expect(page.getByRole("heading", { name: "Issues", exact: true })).toBeVisible({ timeout: 30_000 });
  // The counts are the tracker's own totals (two gh api search reads); the tabs are Open, Closed and All.
  const tabs = page.getByRole("group", { name: "State" }).first();
  await expect(tabs.getByRole("button", { name: "Open · 2" })).toBeVisible();
  await expect(tabs.getByRole("button", { name: "Closed · 1" })).toBeVisible();
  await expect(tabs.getByRole("button", { name: "All", exact: true })).toBeVisible();
  // The default list is the open issues, newest update first, with the tracker's own fields. GitHub issues no cursor, so no
  // Load more row is drawn; paging beyond the first page is not exercised.
  await expect(rows(page)).toHaveCount(2);
  await expect(page.getByRole("button", { name: "Load more", exact: true })).toHaveCount(0);
  const retry = rows(page).filter({ hasText: "Retry the queue" });
  await expect(retry).toContainText("#7 · GitHub · opened by grace");
  await expect(retry.locator(".iss-label")).toHaveText("bug");
  await expect(retry.locator(".iss-state")).toHaveText("Open");
  await expect(retry.locator(".iss-avatar")).toHaveAttribute("title", "me");
  await expect(rows(page).filter({ hasText: "Tidy the docs" })).toContainText("#12 · GitHub · opened by ada");
  // The issue-edit routes are served for GitHub (#1593), so the candidates probe answers 400 invalid (a live route) and the
  // edit and Sort controls are live; the two Sort orders are driven in the Sort test below.
  // Search: the text goes to the tracker as gh --search, one row, a removable chip and the shown line.
  await search(page).fill("queue");
  await search(page).press("Enter");
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Retry the queue");
  await expect(page.getByText("“queue”")).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "1 of 2 open issues" })).toBeVisible();
  await page.locator(".iss-chips").getByRole("button", { name: "Clear filters" }).click();
  await expect(rows(page)).toHaveCount(2);

  // The label filter: the tracker's own label, one row, and the chip removes it.
  await page.getByRole("button", { name: "Filters", exact: true }).click();
  const panel = page.getByRole("dialog", { name: "Filters" });
  await panel.getByRole("group", { name: "Labels" }).getByRole("checkbox", { name: /^bug/ }).check();
  await panel.getByRole("button", { name: "Apply" }).click();
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Retry the queue");
  await page.getByRole("button", { name: "Remove label:bug" }).click();
  await expect(rows(page)).toHaveCount(2);

  // The assignee filter: "Assigned to me" is the gh sign-in's own login (the stand-in's viewer).
  await page.getByRole("button", { name: "Filters", exact: true }).click();
  const again = page.getByRole("dialog", { name: "Filters" });
  await again.getByRole("group", { name: "Assignee" }).getByRole("radio", { name: "Assigned to me" }).check();
  await again.getByRole("button", { name: "Apply" }).click();
  await expect(rows(page)).toHaveCount(1);
  await expect(rows(page).first()).toContainText("Retry the queue");
  await page.getByRole("button", { name: "Remove Assigned to me" }).click();
  await expect(rows(page)).toHaveCount(2);

  // Nothing matches: the page says so in the filter's own words and offers to clear it.
  await search(page).fill("zebra");
  await search(page).press("Enter");
  await expect(page.getByText("No issue matches")).toBeVisible();
  await expect(page.getByText("Nothing in open issues matches “zebra”.")).toBeVisible();
  // The no-match block and the filter chips each offer Clear filters; this is the block's own.
  await page.locator(".iss-nomatch").getByRole("button", { name: "Clear filters" }).click();
  await expect(rows(page)).toHaveCount(2);

  // Closed issues: the tracker date, the Closed word and the closed count.
  await tabs.getByRole("button", { name: "Closed · 1" }).click();
  await expect(rows(page)).toHaveCount(1);
  const old = rows(page).first();
  await expect(old).toContainText("Old cache bug");
  await expect(old.locator(".iss-state")).toHaveText("Closed");
  // The list line carries the tracker's close date as a relative phrase: closed 2 mo. ago for this fixture.
  await expect(old.locator(".iss-sub")).toHaveText(/· closed \d+ [a-z]+\.? ago/);
  await tabs.getByRole("button", { name: "All", exact: true }).click();
  await expect(rows(page)).toHaveCount(3);
});

test("one issue opens as a page with its body, comments and facts, and a thread starts from it", async ({ page }) => {
  await page.goto(`${service.page}#/issues`);
  await expect(rows(page)).toHaveCount(2, { timeout: 30_000 });
  await rows(page).filter({ hasText: "Retry the queue" }).getByRole("button", { name: "Retry the queue" }).click();
  await expect(page).toHaveURL(/#\/issues\/github\/o%2Fr\/7/);
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible();
  await expect(page.locator(".page-sub")).toContainText("#7 · GitHub · o/r");
  await expect(page.locator(".iss-body").first()).toContainText("The queue stalls when the worker restarts.");
  await expect(page.getByText("I can reproduce it.")).toBeVisible();
  const facts = page.getByRole("group", { name: "Facts" });
  await expect(facts).toContainText("GitHub");
  await expect(facts).toContainText("grace");
  await expect(facts).toContainText("bug");
  // Only a plain https address opens, by a click: the Open on GitHub button is the issue's own url.
  await expect(page.getByRole("link", { name: "Open on GitHub" })).toHaveAttribute("href", "https://github.com/o/r/issues/7");
  // No linked pull request is reported, so that card is absent rather than empty.
  await expect(page.getByRole("group", { name: "Linked pull requests" })).toHaveCount(0);

  // Start a thread reads the framed draft from the service (GET /api/issues/draft) and shows it whole, editable.
  await page.getByRole("button", { name: "Start a thread", exact: true }).click();
  const dialog = page.getByRole("dialog", { name: "Start a thread from #7" });
  await expect(dialog).toBeVisible();
  const box = dialog.getByLabel("First message");
  await expect(box).toHaveValue(/Work on #7 "Retry the queue" \(github o\/r, https:\/\/github\.com\/o\/r\/issues\/7\)\./);
  await expect(box).toHaveValue(/<<<ISSUE TEXT\nThe queue stalls when the worker restarts\.\nISSUE TEXT>>>/);
  await expect(dialog.getByLabel("Project")).toHaveValue(projectId);
  const written = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/api/issues/start-thread");
  await dialog.getByRole("button", { name: "Start thread" }).click();
  expect((await written).status()).toBe(200);
  await expect(page).toHaveURL(/#\/threads\//);
  const threadId = threadIdOf(page);
  await settled(service, threadId);
  // One write created the thread, linked it to the issue and queued the edited message.
  const links = await service.api<Record<string, { key: string; display_key: string }[]>>("GET", "/api/issues/links");
  expect(links[threadId]).toHaveLength(1);
  expect(links[threadId]![0]).toMatchObject({ key: "7", display_key: "#7" });
  const messages = JSON.stringify(await service.api("GET", `/api/threads/${threadId}/messages?limit=20`));
  expect(messages).toContain("Work on #7");
  expect(messages).toContain("The queue stalls when the worker restarts.");
});

test("the comment box posts one comment through the real route", async ({ page }) => {
  await page.goto(`${service.page}#/issues/github/o%2Fr/7`);
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible({ timeout: 30_000 });
  // The tracker's own comment is the activity so far.
  await expect(page.getByText("I can reproduce it.", { exact: false })).toBeVisible();
  const form = page.getByRole("form", { name: "Comment" });
  const box = form.getByRole("textbox", { name: "Comment" });
  const body = "Checked against the stand-in.";
  await box.fill(body);
  let comments = 0;
  page.on("request", (request) => { if (request.method() === "POST" && new URL(request.url()).pathname === "/api/issues/item/comment") comments += 1; });
  const written = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/api/issues/item/comment");
  await form.getByRole("button", { name: "Comment", exact: true }).click();
  const answer = await written;
  expect(answer.status()).toBe(200);
  expect(comments).toBe(1);
  expect(answer.request().postDataJSON()).toEqual({ tracker: "github", scope: "o/r", key: "7", body });
  const stored = (await answer.json()) as { id: string; author: { login: string } | null; body: string; created_at: string };
  expect(stored).toMatchObject({ author: { login: "me" }, body });
  expect(stored.id).not.toBe("");
  // One write: the box empties, the answered comment joins the activity, and a detail read of the tracker has it.
  await expect(box).toHaveValue("");
  await expect(page.getByText(body, { exact: true })).toBeVisible();
  const detail = await service.api<{ comments: { author: { login: string } | null; body: string }[] }>("GET", "/api/issues/item?tracker=github&scope=o%2Fr&key=7");
  expect(detail.comments.at(-1)).toMatchObject({ author: { login: "me" }, body });
});

test("the description pencil saves one update through the real route", async ({ page }) => {
  await page.goto(`${service.page}#/issues/github/o%2Fr/7`);
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible({ timeout: 30_000 });
  const pencil = page.getByRole("button", { name: "Edit description" });
  await expect(pencil).toBeEnabled();
  await pencil.click();
  const box = page.getByRole("textbox", { name: "Description" });
  await expect(box).toHaveValue("The queue stalls when the worker restarts.");
  const body = "The queue stalls when the worker restarts. Re-read on the stand-in.";
  await box.fill(body);
  const patches = () => service.ghCalls().filter((call) => call.join(" ") === "api -X PATCH repos/o/r/issues/7 --input -").length;
  const before = patches();
  let updates = 0;
  page.on("request", (request) => { if (request.method() === "POST" && new URL(request.url()).pathname === "/api/issues/item/update") updates += 1; });
  const written = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/api/issues/item/update");
  await page.getByRole("button", { name: "Save", exact: true }).click();
  const answer = await written;
  expect(answer.status()).toBe(200);
  expect(answer.request().postDataJSON()).toEqual({ tracker: "github", scope: "o/r", key: "7", body });
  expect(updates).toBe(1);
  // The editor closes and the page shows the issue the service answered.
  await expect(page.getByRole("textbox", { name: "Description" })).toHaveCount(0);
  await expect(page.locator(".iss-body").first()).toContainText("Re-read on the stand-in.");
  // The write went through gh: exactly one PATCH is recorded, never a hidden second, and a detail read has the new body.
  expect(patches()).toBe(before + 1);
  const detail = await service.api<{ body: string }>("GET", "/api/issues/item?tracker=github&scope=o%2Fr&key=7");
  expect(detail.body).toBe(body);
});

test("the label picker reads the real candidates and applies the whole set, and an unaddressable assignee blocks Apply", async ({ page }) => {
  await page.goto(`${service.page}#/issues/github/o%2Fr/7`);
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible({ timeout: 30_000 });
  const facts = page.getByRole("group", { name: "Facts" });
  const change = page.getByRole("button", { name: "Change labels" });
  await expect(change).toBeEnabled();
  const asked = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/issues/candidates");
  await change.click();
  const answer = await asked;
  expect(answer.status()).toBe(200);
  expect(new URL(answer.url()).searchParams.get("tracker")).toBe("github");
  expect(new URL(answer.url()).searchParams.get("scope")).toBe("o/r");
  // The stand-in's labels and assignable people, as the real candidates route answers them.
  expect(await answer.json()).toEqual({
    labels: [{ name: "bug", color: "d73a4a" }, { name: "docs", color: "0075ca" }],
    assignees: [{ login: "ada", name: null, id: "ada" }, { login: "grace", name: null, id: "grace" }],
    truncated: false,
  });
  const picker = page.getByRole("group", { name: "Change labels" });
  await expect(picker.getByRole("checkbox", { name: "bug" })).toBeChecked();
  await expect(picker.getByRole("checkbox", { name: "docs" })).not.toBeChecked();
  await picker.getByRole("checkbox", { name: "docs" }).check();
  let updates = 0;
  page.on("request", (request) => { if (request.method() === "POST" && new URL(request.url()).pathname === "/api/issues/item/update") updates += 1; });
  const written = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/api/issues/item/update");
  await picker.getByRole("button", { name: "Apply" }).click();
  const update = await written;
  expect(update.status()).toBe(200);
  expect(update.request().postDataJSON()).toEqual({ tracker: "github", scope: "o/r", key: "7", labels: ["bug", "docs"] });
  expect(updates).toBe(1);
  await expect(page.getByRole("group", { name: "Change labels" })).toHaveCount(0);
  await expect(facts.locator(".iss-label")).toHaveCount(2);
  await expect(facts).toContainText("docs");
  // The issue's own assignee "me" is not among the stand-in's assignable people, so the picker says it cannot address it
  // and blocks Apply rather than dropping it silently.
  const people = page.getByRole("button", { name: "Change assignees" });
  await expect(people).toBeEnabled();
  await people.click();
  const who = page.getByRole("group", { name: "Change assignees" });
  await expect(who.getByRole("checkbox", { name: "ada" })).toBeVisible();
  await expect(who.getByRole("checkbox", { name: "me" })).toBeChecked();
  await expect(who.getByRole("alert")).toHaveText("Orbyn cannot address me yet. Uncheck it to continue.");
  await expect(who.getByRole("button", { name: "Apply" })).toBeDisabled();
  await who.getByRole("button", { name: "Cancel" }).click();
});

test("the State menu closes and reopens the issue, one write each way", async ({ page }) => {
  await page.goto(`${service.page}#/issues/github/o%2Fr/7`);
  await expect(page.getByRole("heading", { name: "Retry the queue", exact: true })).toBeVisible({ timeout: 30_000 });
  const patches = () => service.ghCalls().filter((call) => call.join(" ") === "api -X PATCH repos/o/r/issues/7 --input -").length;
  const before = patches();
  let states = 0;
  page.on("request", (request) => { if (request.method() === "POST" && new URL(request.url()).pathname === "/api/issues/item/state") states += 1; });
  // The state is a menu because the detail carries GitHub's own states, in its own order, with the current one marked.
  await page.getByRole("button", { name: "State: Open" }).click();
  const first = page.getByRole("menu", { name: "Change the state" });
  await expect(first.getByRole("menuitem")).toHaveText(["Open", "Closed", "Closed as not planned"]);
  await expect(first.getByRole("menuitem", { name: "Open" })).toHaveAttribute("aria-current", "true");
  const closing = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/api/issues/item/state");
  await first.getByRole("menuitem", { name: "Closed", exact: true }).click();
  const closed = await closing;
  expect(closed.status()).toBe(200);
  expect(closed.request().postDataJSON()).toEqual({ tracker: "github", scope: "o/r", key: "7", state: "Closed", close_reason: "completed" });
  await expect(page.getByRole("button", { name: "State: Closed" })).toBeVisible();
  expect((await service.api<{ state: { category: string; name: string } }>("GET", "/api/issues/item?tracker=github&scope=o%2Fr&key=7")).state).toMatchObject({ category: "closed", name: "Closed" });
  // Reopening is one write again, and the stand-in recorded the one PATCH each way.
  await page.getByRole("button", { name: "State: Closed" }).click();
  const reopening = page.waitForResponse((response) => response.request().method() === "POST" && new URL(response.url()).pathname === "/api/issues/item/state");
  await page.getByRole("menu", { name: "Change the state" }).getByRole("menuitem", { name: "Open", exact: true }).click();
  const reopened = await reopening;
  expect(reopened.status()).toBe(200);
  expect(reopened.request().postDataJSON()).toEqual({ tracker: "github", scope: "o/r", key: "7", state: "Open" });
  await expect(page.getByRole("button", { name: "State: Open" })).toBeVisible();
  expect((await service.api<{ state: { category: string } }>("GET", "/api/issues/item?tracker=github&scope=o%2Fr&key=7")).state.category).toBe("open");
  // Two route writes, two PATCHes: a click that sent a second write each way is caught here.
  expect(states).toBe(2);
  expect(patches()).toBe(before + 2);
});

test("the Sort menu asks the service for each order and each list comes back in it", async ({ page }) => {
  // This write moves #12's updatedAt past #7's; both orders are then observable (#7 was created after #12). The default
  // list key was read by earlier tests and the listing cache is not cleared by a write, so the default read below may be the
  // pre-write answer: its order is not asserted. Setting the text filter writes no query; its submit is the first read of a
  // key that holds this filter and the default (Updated) sort, requested after the write.
  await service.api("POST", "/api/issues/item/update", { tracker: "github", scope: "o/r", key: "12", body: "The Getting started page names the old command. Re-read on the stand-in." });
  await page.goto(`${service.page}#/issues`);
  await expect(page.getByRole("heading", { name: "Issues", exact: true })).toBeVisible({ timeout: 30_000 });
  await expect(rows(page)).toHaveCount(2);
  const filtered = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/issues" && new URL(response.url()).searchParams.get("q") === "the" && new URL(response.url()).searchParams.get("sort") === null);
  await search(page).fill("the");
  await search(page).press("Enter");
  expect((await filtered).status()).toBe(200);
  await expect(rows(page)).toHaveCount(2);
  // Updated is the default sort: the issue written last leads.
  await expect(rows(page).first()).toContainText("Tidy the docs");
  // Sort is live because the candidates probe answered 400 invalid (a live route), not "no such route".
  const sort = page.getByRole("button", { name: "Sort: Updated" });
  await expect(sort).toBeEnabled();
  await sort.click();
  const menu = page.getByRole("menu", { name: "Sort issues" });
  await expect(menu.getByRole("menuitem")).toHaveText(["Updated", "Created"]);
  await expect(menu.getByRole("menuitem", { name: "Updated" })).toHaveAttribute("aria-current", "true");
  // Created is the other order, and this filter+sort key was never requested before the write either.
  const byCreated = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/issues" && new URL(response.url()).searchParams.get("q") === "the" && new URL(response.url()).searchParams.get("sort") === "created");
  await menu.getByRole("menuitem", { name: "Created", exact: true }).click();
  expect((await byCreated).status()).toBe(200);
  await expect(rows(page).first()).toContainText("Retry the queue");
  // The service spells the order qualifier out to gh and sorts its own answer (the stand-in ignores the qualifier).
  const listed = service.ghCalls().filter((args) => args[0] === "issue" && args[1] === "list").at(-1)!;
  expect(listed[listed.indexOf("--search") + 1]).toBe("sort:created-desc");
  // Back to Updated: this key's first read was after the write, so its cached answer holds the post-write order.
  const back = page.waitForResponse((response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/issues" && new URL(response.url()).searchParams.get("q") === "the" && new URL(response.url()).searchParams.get("sort") === "updated");
  await page.getByRole("button", { name: "Sort: Created" }).click();
  await page.getByRole("menu", { name: "Sort issues" }).getByRole("menuitem", { name: "Updated", exact: true }).click();
  expect((await back).status()).toBe(200);
  await expect(rows(page).first()).toContainText("Tidy the docs");
});
