import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock, trackerFixtures } from "../../../mock/issues";
import { useUi } from "../../product/state/ui";
import { MOTION } from "../../lib/motion";
import { setViewportWidth } from "../../test-support";
import { IssuesPage } from "./IssuesPage";
import { setIssuesApi } from "./api";

beforeEach(() => { window.location.hash = "#/issues"; });
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); vi.stubGlobal("__ORBYN_PREVIEWS__", true); vi.useRealTimers(); });

const rows = () => document.querySelectorAll(".iss-rows > li");

it("lists the open issues of every connected tracker, with the tabs counted before any filter", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesPage />);
  expect(await screen.findByRole("button", { name: "Login sends people back to /home" })).toBeVisible();
  expect(rows()).toHaveLength(6);
  expect(screen.getByRole("button", { name: "Open · 6" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: "Closed · 2" })).toBeVisible();
  const first = rows()[0] as HTMLElement;
  expect(within(first).getByText("#412 · GitHub · opened by riley")).toBeVisible();
  expect(within(first).getByText("Open")).toBeVisible();
  expect(within(first).getByText("bug")).toBeVisible();
  expect(screen.getByText("Title and tracker")).toBeVisible();
});

it("opens a row on the detail route", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesPage />);
  fireEvent.click(await screen.findByRole("button", { name: "Login sends people back to /home" }));
  expect(window.location.hash).toBe("#/issues/github/orbyn-dev%2Forbyn-engineering/412");
});

it("shows the closed tab with the closed word and the not planned word", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesPage />);
  fireEvent.click(await screen.findByRole("button", { name: "Closed · 2" }));
  expect(await screen.findByText("Closed as not planned")).toBeVisible();
  expect(rows()).toHaveLength(2);
});

it("scopes the list to one tracker from the picker and says which are not connected", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.click(screen.getByRole("button", { name: /^Tracker/ }));
  const menu = await screen.findByRole("dialog", { name: "Tracker" });
  expect(within(menu).getByText("3 connected")).toBeVisible();
  expect(within(menu).getByText("orbyn-dev/orbyn-engineering · Connected")).toBeVisible();
  expect(within(menu).getAllByRole("button", { name: "Connect" })).toHaveLength(2);
  await user.click(within(menu).getByRole("button", { name: /Linear/ }));
  await waitFor(() => expect(rows()).toHaveLength(2));
  expect(screen.getByText("ORB-87 · Linear · opened by mika")).toBeVisible();
});

it("sends Connect to Settings, Source control, and picking a tracker that is not connected opens the not connected page", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.click(screen.getByRole("button", { name: /^Tracker/ }));
  await user.click(within(await screen.findByRole("dialog", { name: "Tracker" })).getByRole("button", { name: /GitLab/ }));
  expect(await screen.findByRole("heading", { name: "Connect a tracker" })).toBeVisible();
  await user.click(screen.getAllByRole("button", { name: "Connect" })[0]!);
  expect(window.location.hash).toBe("#/settings/source-control");
});

it("shows the connect page, and no list, when no tracker is connected", async () => {
  setIssuesApi(createIssuesMock({ trackers: trackerFixtures().map((row) => ({ ...row, connected: false, scope: null })) }));
  render(<IssuesPage />);
  expect(await screen.findByRole("heading", { name: "Connect a tracker" })).toBeVisible();
  expect(screen.getAllByRole("button", { name: "Connect" })).toHaveLength(5);
  expect(screen.getByText("Orbyn needs one tracker to list issues. Pick the one this project uses. Nothing is posted or changed without you.")).toBeVisible();
  expect(rows()).toHaveLength(0);
});

it("turns the search box and the label word into removable chips with a count and Clear filters", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.type(screen.getByRole("searchbox", { name: "Search issues" }), "reload label:bug{Enter}");
  await waitFor(() => expect(rows()).toHaveLength(1));
  expect(screen.getByText("1 of 6 open issues")).toBeVisible();
  expect(screen.getByRole("button", { name: "Remove label:bug" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Clear filters" }));
  await waitFor(() => expect(rows()).toHaveLength(6));
  expect(screen.queryByText("1 of 6 open issues")).toBeNull();
});

it("filters by label and assignee from the panel, and offers only state, labels and assignee", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.click(screen.getByRole("button", { name: "Filters" }));
  const panel = await screen.findByRole("dialog", { name: "Filters" });
  expect(within(panel).getAllByRole("group").map((group) => group.getAttribute("aria-label"))).toEqual(["State", "Labels", "Assignee"]);
  await user.click(within(panel).getByRole("checkbox", { name: /bug/ }));
  await user.click(within(panel).getByRole("radio", { name: "Unassigned" }));
  await user.click(within(panel).getByRole("button", { name: "Apply" }));
  await waitFor(() => expect(rows()).toHaveLength(1));
  expect(screen.getByRole("button", { name: "Filters · 2" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Remove Unassigned" })).toBeVisible();
});

it("says nothing matches and keeps the one way out", async () => {
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.type(screen.getByRole("searchbox", { name: "Search issues" }), "keyboard label:perf{Enter}");
  expect(await screen.findByText("No issue matches")).toBeVisible();
  expect(screen.getByText("Nothing in open issues matches “keyboard” with the label perf.")).toBeVisible();
  expect(screen.getAllByRole("button", { name: "Clear filters" }).length).toBeGreaterThan(0);
});

it("says which tracker is empty and where to make an issue, with no create button", async () => {
  setIssuesApi(createIssuesMock({ issues: [] }));
  render(<IssuesPage />);
  expect(await screen.findByText("No open issues")).toBeVisible();
  expect(screen.getByText("Issues are created in the tracker itself.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /new issue|create/i })).toBeNull();
});

it("says what failed, that nothing changed, and retries and copies the details", async () => {
  const mock = createIssuesMock();
  mock.failNext("list");
  setIssuesApi(mock);
  const user = userEvent.setup();
  const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
  render(<IssuesPage />);
  expect(await screen.findByText("Can’t reach the tracker")).toBeVisible();
  expect(screen.getByText("The tracker did not answer. Your issues are not changed. Orbyn tries again when you press Retry.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Copy details" }));
  expect(writeText).toHaveBeenCalledWith("GitHub did not answer.");
  await user.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("button", { name: "Login sends people back to /home" })).toBeVisible();
});

it("shows nothing under the header for 400 ms, then quiet bars", async () => {
  vi.useFakeTimers();
  setIssuesApi(createIssuesMock({ delayMs: 5000 }));
  render(<IssuesPage />);
  await act(async () => { await vi.advanceTimersByTimeAsync(10); });
  expect(screen.getByRole("heading", { name: "Issues" })).toBeVisible();
  expect(screen.getByText("Title and tracker")).toBeVisible();
  expect(document.querySelectorAll(".iss-skel > li")).toHaveLength(0);
  await act(async () => { await vi.advanceTimersByTimeAsync(MOTION.wait); });
  expect(document.querySelectorAll(".iss-skel > li").length).toBeGreaterThan(0);
});

it("renders tracker text as text: no HTML, no image request, no link opened by itself", async () => {
  const mock = createIssuesMock({ issues: [{
    tracker: "github", scope: "o/r", key: "1", display_key: "#1", title: "<img src=x onerror=alert(1)> **bold**", url: "https://evil.example/x", author: { login: "<b>riley</b>" },
    state: { category: "open", name: "Open" }, created_at: new Date().toISOString(), updated_at: new Date().toISOString(), assignees: [], labels: [{ name: "<script>x</script>" }], comment_count: 0,
  }] });
  setIssuesApi(mock);
  const open = vi.spyOn(window, "open").mockReturnValue(null);
  render(<IssuesPage />);
  expect(await screen.findByText("<img src=x onerror=alert(1)> **bold**")).toBeVisible();
  expect(document.querySelector(".iss-rows img, .iss-rows script, .iss-rows b")).toBeNull();
  expect(screen.getByText("<script>x</script>")).toBeVisible();
  expect(open).not.toHaveBeenCalled();
});

it("keeps one column on a phone and moves the tracker and filters into one Filters sheet", async () => {
  setViewportWidth(390);
  setIssuesApi(createIssuesMock());
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  expect(screen.queryByRole("button", { name: /^Tracker/ })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Filters" }));
  const sheet = await screen.findByRole("dialog", { name: "Filters" });
  expect(within(sheet).getByRole("group", { name: "Tracker" })).toBeVisible();
  expect(within(sheet).getByRole("group", { name: "Labels" })).toBeVisible();
});

// ---- paging ----

const loadMore = () => screen.queryByRole("button", { name: "Load more" });

it("shows a Load more row only while the answer has next_cursors, and appends the next page with the same filters", async () => {
  const mock = createIssuesMock({ pageSize: 4 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  expect(rows()).toHaveLength(4);
  await user.click(loadMore()!);
  await waitFor(() => expect(rows()).toHaveLength(6));
  expect(loadMore()).toBeNull();
  expect(mock.listed).toHaveLength(2);
  expect(mock.listed[0]!.cursors).toBeUndefined();
  expect(mock.listed[1]!.query).toEqual(mock.listed[0]!.query);
  // The cursor is the service's own, sent back unchanged.
  expect(Object.keys(mock.listed[1]!.cursors!)).toHaveLength(1);
  expect(Object.values(mock.listed[1]!.cursors!)).toEqual(["4"]);
});

it("has no Load more row when the answer has no next_cursors", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  expect(loadMore()).toBeNull();
});

it("one click is one request while a page is loading, and the row says it is busy", async () => {
  const mock = createIssuesMock({ pageSize: 4, delayMs: 30 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.dblClick(loadMore()!);
  expect(screen.getByRole("button", { name: "Load more" })).toHaveAttribute("aria-busy", "true");
  await waitFor(() => expect(rows()).toHaveLength(6));
  expect(mock.listed).toHaveLength(2);
});

it("drops a page that lands after the filter changed: the latest request wins", async () => {
  const mock = createIssuesMock({ pageSize: 4, delayMs: 40 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  await user.click(loadMore()!);
  await user.click(screen.getByRole("button", { name: /^Closed/ }));
  await waitFor(() => expect(screen.getByText("Closed as not planned")).toBeVisible());
  await new Promise((resolve) => setTimeout(resolve, 120));
  expect(rows()).toHaveLength(2);
  expect(loadMore()).toBeNull();
});

it("says so when a page cannot be loaded, keeps what is shown and keeps the row", async () => {
  const mock = createIssuesMock({ pageSize: 4 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesPage />);
  await screen.findByRole("button", { name: "Login sends people back to /home" });
  mock.failNext("list");
  await user.click(loadMore()!);
  await waitFor(() => expect(useUi.getState().toasts.some((toast) => /Couldn.t load more/.test(toast.text))).toBe(true));
  expect(rows()).toHaveLength(4);
  expect(loadMore()).toBeEnabled();
});

// ---- state change is Planned ----

it("in a production build the State control is Planned text, and no choice is offered or sent even when the answer carries transitions", async () => {
  vi.stubGlobal("__ORBYN_PREVIEWS__", false);
  const mock = createIssuesMock();
  setIssuesApi(mock);
  window.location.hash = "#/issues/github/orbyn-dev%2Forbyn-engineering/412";
  render(<IssuesPage />);
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).getByText("Planned")).toBeVisible();
  expect(within(facts).getByText("Open")).toBeVisible();
  expect(screen.queryByRole("button", { name: /^State:/ })).toBeNull();
  expect(mock.writes).toEqual([]);
});
