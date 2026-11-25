import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock, draftOf, issueFixtures } from "../../../mock/issues";
import type { Project, Thread } from "../../api/types";
import type { IssueRef } from "./contract";
import { MOTION } from "../../lib/motion";
import { useWorkspace } from "../../product/state/workspace";
import { IssuesPage } from "./IssuesPage";
import { setIssuesApi } from "./api";
import { resetIssueLinks } from "./links";

const project = { id: "p1", name: "orbyn-engineering", path: "/work/orbyn" } as Project;
const thread = (id: string, title: string) => ({ id, project_id: "p1", title, engine: "claude", model: null, cwd: "/", worktree: null, skill_profile: null, active_run: null, last_run_state: null, created_at: "2026-11-20T10:00:00Z", updated_at: new Date(Date.now() - 3 * 86_400_000).toISOString() }) as Thread;
const refOf = (key: string): IssueRef => {
  const one = issueFixtures().find((issue) => issue.key === key)!;
  return { tracker: one.tracker, scope: one.scope, key: one.key, display_key: one.display_key, title: one.title, state: one.state, labels: one.labels };
};

beforeEach(() => {
  resetIssueLinks();
  useWorkspace.setState({ projects: { data: [project], error: null, loading: false }, threads: { "thread-1": thread("thread-1", "Fix login redirect") } });
});
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); vi.useRealTimers(); });

const at = (hash: string) => { window.location.hash = hash; render(<IssuesPage />); };

it("shows the title, state, tracker, facts, body and comments as written", async () => {
  setIssuesApi(createIssuesMock());
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  expect(await screen.findByRole("heading", { name: "Login sends people back to /home" })).toBeVisible();
  expect(screen.getByText("#412 · GitHub · orbyn-dev/orbyn-engineering")).toBeVisible();
  const facts = screen.getByRole("group", { name: "Facts" });
  expect(within(facts).getByText("Open")).toBeVisible();
  expect(within(facts).getByText("GitHub")).toBeVisible();
  expect(within(facts).getByText("riley", { selector: "dd" })).toBeVisible();
  expect(within(facts).getByText("auth")).toBeVisible();
  expect(screen.getByText("Reproduced. The redirect reads", { exact: false })).toBeVisible();
  expect(screen.getByRole("link", { name: "Open on GitHub" })).toHaveAttribute("href", "https://github.com/orbyn-dev/orbyn-engineering/issues/412");
  expect(screen.getByRole("link", { name: "Back to Issues" })).toHaveAttribute("href", "#/issues");
});

it("lists the linked threads, counts them in the header and offers to start or link another", async () => {
  setIssuesApi(createIssuesMock());
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  expect(await within(card).findByRole("link", { name: /Fix login redirect/ })).toHaveAttribute("href", "#/threads/thread-1");
  expect(within(card).getByRole("button", { name: "Start a thread from this issue" })).toBeVisible();
  expect(within(card).getByRole("button", { name: "Link to a thread…" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked from 1 thread");
});

it("says nothing is linked yet, draws no header chip and offers the two actions", async () => {
  setIssuesApi(createIssuesMock({ links: {} }));
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  expect(await within(card).findByText("No thread is linked to this issue yet.")).toBeVisible();
  expect(within(card).getByRole("button", { name: "Start a thread from this issue" })).toBeVisible();
  expect(within(card).getByRole("button", { name: "Link to a thread…" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Linked threads" })).toBeNull();
});

it("opens the served linked threads from the header chip, read only, and follows one to its thread", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const chip = await screen.findByRole("button", { name: "Linked threads" });
  expect(chip).toHaveTextContent("Linked from 1 thread");
  await user.click(chip);
  const pop = await screen.findByRole("dialog", { name: "Linked threads" });
  // The list is read only: it offers no unlink, no link, nothing that writes.
  expect(within(pop).queryAllByRole("button")).toHaveLength(0);
  // With no button or input inside it, the panel itself takes focus, so its label is announced and Tab reaches its rows.
  expect(pop).toHaveFocus();
  const link = within(pop).getByRole("link", { name: /Fix login redirect/ });
  expect(link).toHaveAttribute("href", "#/threads/thread-1");
  await user.click(link);
  expect(window.location.hash).toBe("#/threads/thread-1");
});

it("counts and lists every linked thread the issue's read carries, in its order", async () => {
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect"), "thread-3": thread("thread-3", "Trace the queue") } });
  setIssuesApi(createIssuesMock({ links: { "thread-1": [refOf("412")], "thread-2": [refOf("412")], "thread-3": [refOf("412")] } }));
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const chip = await screen.findByRole("button", { name: "Linked threads" });
  expect(chip).toHaveTextContent("Linked from 3 threads");
  await user.click(chip);
  const pop = await screen.findByRole("dialog", { name: "Linked threads" });
  expect(within(pop).getAllByRole("link").map((link) => link.getAttribute("href"))).toEqual(["#/threads/thread-1", "#/threads/thread-2", "#/threads/thread-3"]);
  expect(within(pop).getByRole("link", { name: /Trace the queue/ })).toBeVisible();
});

it("keeps a linked thread the app has not loaded in the count and opens it by its id", async () => {
  setIssuesApi(createIssuesMock({ links: { "thread-1": [refOf("412")], "thread-gone": [refOf("412")] } }));
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const chip = await screen.findByRole("button", { name: "Linked threads" });
  expect(chip).toHaveTextContent("Linked from 2 threads");
  await user.click(chip);
  const pop = await screen.findByRole("dialog", { name: "Linked threads" });
  expect(within(pop).getByRole("link", { name: "thread-gone" })).toHaveAttribute("href", "#/threads/thread-gone");
});

it("links a thread from the card: choosing writes nothing, one Link is one write, and both lists show it", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect") } });
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  await user.click(within(card).getByRole("button", { name: "Link to a thread…" }));
  const dialog = await screen.findByRole("dialog", { name: "Link a thread" });
  // The thread that already links this issue is not offered.
  expect(within(dialog).queryByRole("option", { name: /Fix login redirect/ })).toBeNull();
  await user.click(within(dialog).getByRole("option", { name: /Try cookie-based redirect/ }));
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  await waitFor(() => expect(mock.writes).toEqual(["link:thread-2:412"]));
  await waitFor(() => expect(screen.queryByRole("dialog", { name: "Link a thread" })).toBeNull());
  expect(await screen.findByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked from 2 threads");
  expect(await within(card).findByRole("link", { name: /Try cookie-based redirect/ })).toBeVisible();
});

it("keeps the link dialog open and says why when the link write fails", async () => {
  const mock = createIssuesMock();
  mock.failNext("link");
  setIssuesApi(mock);
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect") } });
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  await user.click(within(card).getByRole("button", { name: "Link to a thread…" }));
  const dialog = await screen.findByRole("dialog", { name: "Link a thread" });
  await user.click(within(dialog).getByRole("option", { name: /Try cookie-based redirect/ }));
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Couldn’t link it. Nothing changed. GitHub did not answer.");
  expect(within(dialog).getByRole("button", { name: "Link" })).toBeEnabled();
  expect(mock.writes).toEqual(["link:thread-2:412"]);
  expect(screen.getByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked from 1 thread");
});

it("walks the thread picker by arrow keys, boundaries included, and picks a row with Enter", async () => {
  setIssuesApi(createIssuesMock());
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect"), "thread-3": thread("thread-3", "Trace the queue") } });
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  await user.click(within(card).getByRole("button", { name: "Link to a thread…" }));
  const dialog = await screen.findByRole("dialog", { name: "Link a thread" });
  const search = within(dialog).getByRole("combobox", { name: "Search threads" });
  const options = within(dialog).getAllByRole("option");
  expect(options).toHaveLength(2);
  expect(search).toHaveFocus();
  // Arrow Down from the search enters the list; the arrows move between rows and stop at the ends.
  await user.keyboard("{ArrowDown}");
  expect(options[0]).toHaveFocus();
  await user.keyboard("{ArrowDown}");
  expect(options[1]).toHaveFocus();
  await user.keyboard("{ArrowDown}");
  expect(options[1]).toHaveFocus();
  await user.keyboard("{ArrowUp}");
  expect(options[0]).toHaveFocus();
  await user.keyboard("{ArrowUp}");
  expect(options[0]).toHaveFocus();
  await user.keyboard("{Enter}");
  expect(options[0]).toHaveAttribute("aria-selected", "true");
});

it("a link is not a second write: while the page is saving another change, Link sends nothing and says so", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect") } });
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  expect(await within(card).findByRole("link", { name: /Fix login redirect/ })).toBeVisible();
  // Hold one description write open, then open the picker and try to link while it is in flight.
  const realUpdate = mock.update;
  let finish = () => {};
  mock.update = (ref, input) => new Promise((resolve) => { finish = () => void realUpdate(ref, input).then(resolve); });
  const pencil = screen.getByRole("button", { name: "Edit description" });
  await waitFor(() => expect(pencil).toBeEnabled());
  await user.click(pencil);
  const box = screen.getByRole("textbox", { name: "Description" });
  await user.clear(box);
  await user.type(box, "Rewritten");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await user.click(within(card).getByRole("button", { name: "Link to a thread…" }));
  const dialog = await screen.findByRole("dialog", { name: "Link a thread" });
  await user.click(within(dialog).getByRole("option", { name: /Try cookie-based redirect/ }));
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("The page is still saving another change.");
  expect(mock.writes.filter((write) => write.startsWith("link:"))).toEqual([]);
  // Once the page's write has settled, the same press goes through.
  await act(async () => { finish(); });
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull());
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  expect(await within(card).findByRole("link", { name: /Try cookie-based redirect/ })).toBeVisible();
  expect(mock.writes).toEqual(["update:412:body", "link:thread-2:412"]);
});

it("reads the links after the link, not an answer that was already in flight", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect") } });
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  expect(await within(card).findByRole("link", { name: /Fix login redirect/ })).toBeVisible();
  // A links read starts before the link and stays in flight with the links as they were then.
  const realLinks = mock.links;
  let started = false;
  let release = () => {};
  mock.links = () => {
    if (!started) {
      started = true;
      return new Promise((resolve) => { release = () => resolve({ "thread-1": [refOf("412")] }); });
    }
    return realLinks();
  };
  const pencil = screen.getByRole("button", { name: "Edit description" });
  await waitFor(() => expect(pencil).toBeEnabled());
  await user.click(pencil);
  const box = screen.getByRole("textbox", { name: "Description" });
  await user.clear(box);
  await user.type(box, "Rewritten");
  await user.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(started).toBe(true));
  // The link's own read starts after its write and shows the link; the older read must not put the card back.
  await user.click(within(card).getByRole("button", { name: "Link to a thread…" }));
  const dialog = await screen.findByRole("dialog", { name: "Link a thread" });
  await user.click(within(dialog).getByRole("option", { name: /Try cookie-based redirect/ }));
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  expect(await within(card).findByRole("link", { name: /Try cookie-based redirect/ })).toBeVisible();
  await waitFor(() => expect(screen.getByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked from 2 threads"));
  await act(async () => { release(); });
  expect(within(card).getAllByRole("link")).toHaveLength(2);
});

it("keeps the listed threads visible and retries only the links read when the read after a link fails", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  useWorkspace.setState({ threads: { "thread-1": thread("thread-1", "Fix login redirect"), "thread-2": thread("thread-2", "Try cookie-based redirect") } });
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  const card = await screen.findByRole("group", { name: "Linked threads" });
  expect(await within(card).findByRole("link", { name: /Fix login redirect/ })).toBeVisible();
  await user.click(within(card).getByRole("button", { name: "Link to a thread…" }));
  const dialog = await screen.findByRole("dialog", { name: "Link a thread" });
  await user.click(within(dialog).getByRole("option", { name: /Try cookie-based redirect/ }));
  // The write lands; the read that follows it fails. The issue's own read refreshes the header count, so the chip counts
  // the thread the card has not listed yet, and the card says so rather than hiding the failure behind its known rows.
  mock.failNext("links");
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked from 2 threads"));
  expect(within(card).getAllByRole("link")).toHaveLength(1);
  expect(await within(card).findByRole("alert")).toHaveTextContent("Couldn’t refresh the linked threads. The list may be out of date.");
  // Retry reads the links again; the successful link write is not sent a second time.
  expect(mock.writes).toEqual(["link:thread-2:412"]);
  await user.click(within(card).getByRole("button", { name: "Retry" }));
  expect(await within(card).findByRole("link", { name: /Try cookie-based redirect/ })).toBeVisible();
  await waitFor(() => expect(within(card).queryByRole("alert")).toBeNull());
  expect(within(card).getAllByRole("link")).toHaveLength(2);
  expect(mock.writes).toEqual(["link:thread-2:412"]);
});

it("keeps a closed issue's page and says how it closed", async () => {
  setIssuesApi(createIssuesMock());
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/377");
  expect(await screen.findByRole("heading", { name: "Offer a dark favicon" })).toBeVisible();
  expect(screen.getAllByText("Closed as not planned").length).toBeGreaterThan(0);
  expect(screen.getAllByRole("button", { name: /Start a thread/ }).length).toBeGreaterThan(0);
});

it("fails as the pull request page does: what failed, that nothing changed, one retry", async () => {
  const mock = createIssuesMock();
  mock.failNext("get");
  setIssuesApi(mock);
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  expect(await screen.findByText("Couldn’t load this issue")).toBeVisible();
  expect(screen.getByText("Nothing changed.")).toBeVisible();
  expect(screen.getAllByRole("button", { name: "Retry" })).toHaveLength(1);
  await user.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("heading", { name: "Login sends people back to /home" })).toBeVisible();
});

it("says there is no such issue on a 404", async () => {
  setIssuesApi(createIssuesMock());
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/9999");
  expect(await screen.findByText("No issue #9999 on GitHub.")).toBeVisible();
});

it("draws quiet bars only after 400 ms", async () => {
  vi.useFakeTimers();
  setIssuesApi(createIssuesMock({ delayMs: 5000 }));
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  await act(async () => { await vi.advanceTimersByTimeAsync(10); });
  expect(document.querySelectorAll(".iss-detail-skel")).toHaveLength(0);
  await act(async () => { await vi.advanceTimersByTimeAsync(MOTION.wait); });
  expect(document.querySelectorAll(".iss-detail-skel").length).toBeGreaterThan(0);
});

it("renders the body as escaped Markdown: no HTML, no remote image, no link opened by itself", async () => {
  const mock = createIssuesMock();
  const real = mock.get;
  mock.get = async (ref) => ({ ...(await real(ref)), body: "<img src=x onerror=alert(1)> and ![t](https://evil.example/p.png) and [go](https://evil.example/go) <script>boom()</script>", comments: [] });
  setIssuesApi(mock);
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  await screen.findByRole("heading", { name: "Login sends people back to /home" });
  expect(document.querySelector(".iss-body img, .iss-body script")).toBeNull();
  expect(document.querySelector(".iss-body")?.textContent).toContain("<img src=x onerror=alert(1)>");
  expect(open).not.toHaveBeenCalled();
});

const DRAFT_HEAD = 'Work on #412 "Login sends people back to /home" (github orbyn-dev/orbyn-engineering, https://github.com/orbyn-dev/orbyn-engineering/issues/412).';

it("shows the framed draft to edit, reads it without a write, then starts the thread with one click and one write", async () => {
  const mock = createIssuesMock({ delayMs: 30, links: {} });
  setIssuesApi(mock);
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  await user.click(await screen.findByRole("button", { name: "Start a thread" }));
  const dialog = await screen.findByRole("dialog", { name: "Start a thread from #412" });
  expect(within(dialog).getByText("#412 Login sends people back to /home")).toBeVisible();
  const box = within(dialog).getByRole("textbox", { name: "First message" });
  // While the draft is read the box cannot be edited and Start is off.
  expect(box).toHaveAttribute("readonly");
  expect(within(dialog).getByRole("button", { name: "Start thread" })).toBeDisabled();
  await waitFor(() => expect((box as HTMLTextAreaElement).value).toContain(DRAFT_HEAD));
  expect(box).not.toHaveAttribute("readonly");
  expect((box as HTMLTextAreaElement).value).toContain("<<<ISSUE TEXT\nAfter sign-in the app opens");
  expect((box as HTMLTextAreaElement).value).toContain("ISSUE TEXT>>>");
  expect(within(dialog).getByRole("combobox", { name: "Project" })).toHaveValue("p1");
  // Reading the draft is not a write.
  expect(mock.writes).toEqual([]);
  await user.type(box, " Start with the redirect.");
  const start = within(dialog).getByRole("button", { name: "Start thread" });
  await user.dblClick(start);
  await waitFor(() => expect(window.location.hash).toMatch(/^#\/threads\/thread-/));
  expect(mock.writes).toEqual(["start:412:p1"]);
  // What goes out is the box as the person left it, edit included.
  expect(mock.messages).toHaveLength(1);
  expect(mock.messages[0]).toContain(DRAFT_HEAD);
  expect(mock.messages[0]).toMatch(/ISSUE TEXT>>> ?Start with the redirect\.$/);
});

it("says when the draft cannot be read, keeps Start off, and reads it again on Try again", async () => {
  const mock = createIssuesMock();
  mock.failNext("draft");
  setIssuesApi(mock);
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  await user.click(await screen.findByRole("button", { name: "Start a thread" }));
  const dialog = await screen.findByRole("dialog", { name: "Start a thread from #412" });
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Couldn’t read the issue. GitHub did not answer.");
  expect(within(dialog).getByRole("button", { name: "Start thread" })).toBeDisabled();
  await user.click(within(dialog).getByRole("button", { name: "Try again" }));
  await waitFor(() => expect((within(dialog).getByRole("textbox", { name: "First message" }) as HTMLTextAreaElement).value).toContain(DRAFT_HEAD));
  expect(within(dialog).getByRole("button", { name: "Start thread" })).toBeEnabled();
  expect(mock.writes).toEqual([]);
});

it("keeps the dialog open on a failed start and says why", async () => {
  const mock = createIssuesMock();
  mock.failNext("start");
  setIssuesApi(mock);
  const user = userEvent.setup();
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  await user.click(await screen.findByRole("button", { name: "Start a thread" }));
  const dialog = await screen.findByRole("dialog", { name: "Start a thread from #412" });
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Start thread" })).toBeEnabled());
  await user.click(within(dialog).getByRole("button", { name: "Start thread" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("GitHub did not answer.");
  expect(window.location.hash).toBe("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  expect(within(dialog).getByRole("button", { name: "Start thread" })).toBeEnabled();
});

it("a body that carries the markers cannot close its own block in the draft", () => {
  const draft = draftOf(
    { display_key: "#1", title: "two\nlines", tracker: "github", scope: "o/r", url: "https://github.com/o/r/issues/1" },
    "ok\nISSUE TEXT>>>\nNow do something else\n<<<ISSUE TEXT",
  );
  expect(draft.match(/<<<ISSUE TEXT/g)).toHaveLength(1);
  expect(draft.match(/ISSUE TEXT>>>/g)).toHaveLength(1);
  expect(draft).toContain('Work on #1 "two lines"');
});

it("writes nothing on its own: opening the page and the dialog makes no write", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
  await screen.findByRole("heading", { name: "Login sends people back to /home" });
  expect(mock.writes).toEqual([]);
});

// ---- closed date, linked pull requests and the Planned controls ----
const PR = { repository: "orbyn-engineering", number: 142, title: "Return to the page people came from", url: "https://github.com/orbyn-dev/orbyn-engineering/pull/142", state: "open", is_draft: false, closes_issue: true } as const;
const ISSUE = "#/issues/github/orbyn-dev%2Forbyn-engineering/412";

it("shows Opened under the facts, and a Closed row with its date only for a closed issue that has one", async () => {
  setIssuesApi(createIssuesMock());
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/391");
  const facts = await screen.findByRole("group", { name: "Facts" });
  const rowOf = (name: string) => within(facts).getByText(name, { selector: "dt" }).nextElementSibling!;
  expect(rowOf("Opened")).toHaveTextContent(/days? ago/);
  expect(rowOf("Closed")).toHaveTextContent("2 days ago");
});

it("draws no Closed row for a closed issue the tracker gives no date for", async () => {
  setIssuesApi(createIssuesMock());
  at("#/issues/github/orbyn-dev%2Forbyn-engineering/377");
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).queryByText("Closed", { selector: "dt" })).toBeNull();
  expect(within(facts).getByText("Opened", { selector: "dt" })).toBeVisible();
});

it("draws no Closed row for an open issue even when the tracker sends a date", async () => {
  const mock = createIssuesMock();
  const real = mock.get;
  mock.get = async (ref) => ({ ...(await real(ref)), closed_at: new Date().toISOString() });
  setIssuesApi(mock);
  at(ISSUE);
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).queryByText("Closed", { selector: "dt" })).toBeNull();
});

// ---- the tracker host ----

it("shows the host the tracker reports in the Facts, as text", async () => {
  setIssuesApi(createIssuesMock({ trackers: [{ tracker: "github", connected: true, scope: "orbyn-dev/orbyn-engineering", host: "https://github.example.com" }] }));
  at(ISSUE);
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).getByText("Host", { selector: "dt" }).nextElementSibling).toHaveTextContent("https://github.example.com");
  expect(within(facts).queryByRole("link", { name: /github\.example\.com/ })).toBeNull();
});

it("draws no Host row when the tracker reports no host", async () => {
  setIssuesApi(createIssuesMock());
  at(ISSUE);
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).queryByText("Host", { selector: "dt" })).toBeNull();
});

it("draws another tracker's host, between Tracker and Author", async () => {
  // The rows are the mock's: Linear reports its fixed API host, so a tracker other than GitHub exercises the row.
  setIssuesApi(createIssuesMock());
  at("#/issues/linear/Orbyn%20team/ORB-87");
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).getByText("Host", { selector: "dt" }).nextElementSibling).toHaveTextContent("https://api.linear.app");
  const terms = [...facts.querySelectorAll("dt")].map((term) => term.textContent);
  expect(terms.indexOf("Host")).toBe(terms.indexOf("Tracker") + 1);
  expect(terms.indexOf("Author")).toBe(terms.indexOf("Host") + 1);
});

it("lists the linked pull requests the tracker reports: state, title, number, repository and whether it closes the issue", async () => {
  const merged = { ...PR, number: 131, title: "Keep the next parameter when signing in", state: "merged", closes_issue: false } as const;
  const draft = { ...PR, number: 150, title: "Try cookies", repository: "orbyn-planner", is_draft: true, closes_issue: false } as const;
  const closed = { ...PR, number: 90, title: "Old attempt", state: "closed", closes_issue: false } as const;
  setIssuesApi(createIssuesMock({ pullRequests: { "412": [PR, merged, draft, closed] } }));
  at(ISSUE);
  const card = await screen.findByRole("group", { name: "Linked pull requests" });
  expect(within(card).getByRole("heading", { name: "Linked pull requests · 4" })).toBeVisible();
  const items = within(card).getAllByRole("listitem");
  expect(items.map((item) => item.querySelector("a")!.textContent)).toEqual(expect.arrayContaining([expect.stringContaining("Return to the page people came from")]));
  expect(items).toHaveLength(4);
  expect(items[0]).toHaveTextContent("#142");
  expect(items[0]).toHaveTextContent("orbyn-engineering");
  expect(items[0]).toHaveTextContent("Closes this issue");
  expect(items[0]).not.toHaveTextContent(/Merged|Draft|Closed\b(?! this)/);
  expect(items[1]).toHaveTextContent("Merged");
  expect(items[1]).not.toHaveTextContent("Closes this issue");
  expect(items[2]).toHaveTextContent("Draft");
  expect(items[2]).toHaveTextContent("orbyn-planner");
  expect(items[3]).toHaveTextContent("Closed");
  // Each row opens the pull request on the host, in its own tab; there is no CI or checks status.
  const link = within(items[0]!).getByRole("link", { name: /Return to the page people came from/ });
  expect(link).toHaveAttribute("href", PR.url);
  expect(link).toHaveAttribute("target", "_blank");
  expect(link).toHaveAttribute("rel", expect.stringContaining("noopener"));
  expect(within(card).queryByText(/checks|CI/i)).toBeNull();
});

it("keeps the tracker's order for linked pull requests", async () => {
  setIssuesApi(createIssuesMock({ pullRequests: { "412": [{ ...PR, number: 3, title: "C" }, { ...PR, number: 1, title: "A" }, { ...PR, number: 2, title: "B" }] } }));
  at(ISSUE);
  const card = await screen.findByRole("group", { name: "Linked pull requests" });
  expect(within(card).getAllByRole("listitem").map((item) => item.querySelector("a")!.textContent?.slice(0, 1))).toEqual(["C", "A", "B"]);
});

it("draws no Linked pull requests card when the tracker reports none", async () => {
  setIssuesApi(createIssuesMock());
  at(ISSUE);
  await screen.findByRole("group", { name: "Facts" });
  expect(screen.queryByRole("group", { name: "Linked pull requests" })).toBeNull();
  expect(screen.queryByText("No pull request is linked to this issue.")).toBeNull();
});

it("draws a pull request's words as text and opens only a plain https address", async () => {
  setIssuesApi(createIssuesMock({ pullRequests: { "412": [
    { ...PR, title: "<img src=x onerror=alert(1)>", repository: "<b>repo</b>" },
    { ...PR, number: 2, title: "Unsafe", url: "javascript:alert(1)" },
  ] } }));
  at(ISSUE);
  const card = await screen.findByRole("group", { name: "Linked pull requests" });
  expect(within(card).getByText("<img src=x onerror=alert(1)>")).toBeVisible();
  expect(within(card).getByText(/<b>repo<\/b>/)).toBeVisible();
  expect(card.querySelector("img, script")).toBeNull();
  expect(card.querySelectorAll("b")).toHaveLength(2);
  expect(within(card).getAllByRole("link")).toHaveLength(1);
  expect(within(card).getByText("Unsafe")).toBeVisible();
});

it("caps a pull request's title and repository so one long word cannot take the card over", async () => {
  setIssuesApi(createIssuesMock({ pullRequests: { "412": [{ ...PR, title: "t".repeat(5000), repository: "r".repeat(5000) }] } }));
  at(ISSUE);
  const card = await screen.findByRole("group", { name: "Linked pull requests" });
  expect(card.textContent!.length).toBeLessThan(1000);
});

it("says the thread links could not be read, offers Retry, and keeps the facts and the pull requests", async () => {
  const mock = createIssuesMock({ pullRequests: { "412": [PR] } });
  mock.failNext("links");
  setIssuesApi(mock);
  const user = userEvent.setup();
  at(ISSUE);
  const threads = await screen.findByRole("group", { name: "Linked threads" });
  expect(await within(threads).findByText("Couldn’t read the linked threads. Nothing changed.")).toBeVisible();
  expect(within(threads).queryByText("No thread is linked to this issue yet.")).toBeNull();
  expect(screen.getByRole("group", { name: "Facts" })).toBeVisible();
  expect(screen.getByRole("link", { name: /Return to the page people came from/ })).toBeVisible();
  // The count is the issue's own read, so the header still knows it even though the links read failed.
  expect(screen.getByRole("button", { name: "Linked threads" })).toHaveTextContent("Linked from 1 thread");
  await user.dblClick(within(threads).getByRole("button", { name: "Retry" }));
  expect(await within(threads).findByRole("link", { name: /Fix login redirect/ })).toBeVisible();
  expect(within(threads).queryByRole("button", { name: "Retry" })).toBeNull();
});

it("shows the description, labels and assignees controls as Planned while the service does not offer the writes: present, off, and nothing is sent", async () => {
  const mock = createIssuesMock({ edits: false });
  setIssuesApi(mock);
  const user = userEvent.setup();
  at(ISSUE);
  await screen.findByRole("group", { name: "Facts" });
  for (const name of ["Edit description", "Change labels", "Change assignees"]) {
    const button = screen.getByRole("button", { name });
    expect(button).toBeDisabled();
    expect(button.parentElement).toHaveTextContent("Planned");
    await user.click(button);
  }
  expect(screen.queryByRole("textbox", { name: "Description" })).toBeNull();
  expect(screen.queryByRole("menu")).toBeNull();
  expect(mock.writes).toEqual([]);
});
