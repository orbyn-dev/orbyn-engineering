import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createIssuesMock, issueFixtures } from "../../../mock/issues";
import type { Project, Thread } from "../../api/types";
import { fileTabId, parseFileTab, useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { tabOf } from "../panels/strip/tabs";
import { IssuePanel } from "./IssuePanel";
import { IssuesPage } from "./IssuesPage";
import { IssuesRow } from "./ThreadIssues";
import { setIssuesApi } from "./api";
import type { IssueRef } from "./contract";
import { resetIssueLinks } from "./links";

const project = { id: "p1", name: "orbyn-engineering", path: "/work/orbyn" } as Project;
const thread = { id: "thread-1", project_id: "p1", title: "Fix login redirect", engine: "claude", cwd: "/", worktree: null } as Thread;
const refOf = (key: string): IssueRef => {
  const one = issueFixtures().find((issue) => issue.key === key)!;
  return { tracker: one.tracker, scope: one.scope, key: one.key, display_key: one.display_key, title: one.title, state: one.state, labels: one.labels, comment_count: one.comment_count };
};

beforeEach(() => {
  resetIssueLinks();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [project], error: null, loading: false }, threads: { "thread-1": thread } });
});
afterEach(() => { setIssuesApi(null); window.location.hash = ""; vi.restoreAllMocks(); });

// ---- the state menu on an issue's page ----

const stateButton = async (name = /^State:/) => screen.findByRole("button", { name });

it("lists the states GitHub offers, marks the current one, and one choice is one write", async () => {
  const mock = createIssuesMock({ delayMs: 20 });
  setIssuesApi(mock);
  window.location.hash = "#/issues/github/412";
  const user = userEvent.setup();
  render(<IssuesPage />);
  const button = await stateButton();
  expect(button).toHaveAccessibleName("State: Open");
  await user.click(button);
  const menu = await screen.findByRole("menu", { name: "Change the state" });
  const items = within(menu).getAllByRole("menuitem");
  expect(items.map((item) => item.textContent)).toEqual(["Open", "Closed", "Closed as not planned"]);
  expect(items[0]).toHaveAttribute("aria-current", "true");
  await user.dblClick(items[1]!);
  await waitFor(() => expect(screen.getByRole("button", { name: "State: Closed" })).toBeVisible());
  expect(mock.writes).toEqual(["state:412:Closed:completed"]);
});

it("choosing the current state writes nothing", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  window.location.hash = "#/issues/github/412";
  const user = userEvent.setup();
  render(<IssuesPage />);
  await user.click(await stateButton());
  await user.click(within(await screen.findByRole("menu")).getAllByRole("menuitem")[0]!);
  expect(mock.writes).toEqual([]);
});

it("a tracker with a workflow lists its own names", async () => {
  setIssuesApi(createIssuesMock());
  window.location.hash = "#/issues/linear/ORB-87";
  const user = userEvent.setup();
  render(<IssuesPage />);
  await user.click(await stateButton());
  expect(within(await screen.findByRole("menu")).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Todo", "In review", "Done"]);
});

it("a tracker that cannot change state shows the state as text, with no button", async () => {
  setIssuesApi(createIssuesMock({ fixedState: ["github"] }));
  window.location.hash = "#/issues/github/412";
  render(<IssuesPage />);
  const facts = await screen.findByRole("group", { name: "Facts" });
  expect(within(facts).getByText("Open")).toBeVisible();
  expect(screen.queryByRole("button", { name: /^State:/ })).toBeNull();
});

it("a refused change says nothing changed and leaves the state as it was", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  mock.failNext("state");
  window.location.hash = "#/issues/github/412";
  const user = userEvent.setup();
  render(<IssuesPage />);
  await user.click(await stateButton());
  await user.click(within(await screen.findByRole("menu")).getAllByRole("menuitem")[2]!);
  await waitFor(() => expect(useUi.getState().toasts.some((toast) => /Couldn.t change the state\. Nothing changed\./.test(toast.text))).toBe(true));
  expect(screen.getByRole("button", { name: "State: Open" })).toBeEnabled();
});

// ---- the Details card's Issues row ----

it("says None linked with Link an issue…, and nothing at all while the service reports no trackers", async () => {
  setIssuesApi(null);
  const { unmount } = render(<IssuesRow thread={thread} />);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Issues")).toBeNull();
  unmount();
  setIssuesApi(createIssuesMock({ links: {} }));
  render(<IssuesRow thread={thread} />);
  expect(await screen.findByText("None linked")).toBeVisible();
  expect(screen.getByRole("button", { name: "Link an issue…" })).toBeVisible();
});

it("lists one line for one issue: tracker, key, state word, title and Unlink, and no Link line", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesRow thread={thread} />);
  const line = await screen.findByRole("button", { name: "#412 Login sends people back to /home" });
  expect(line).toHaveTextContent("GitHub · #412");
  expect(line).toHaveTextContent("Open");
  expect(screen.getByRole("button", { name: "Unlink #412" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Link an issue…" })).toBeNull();
});

it("lists several across trackers, closed ones kept with their own words, and offers Link an issue…", async () => {
  setIssuesApi(createIssuesMock({ links: { "thread-1": [refOf("412"), refOf("ORB-87"), refOf("391"), refOf("377")] } }));
  render(<IssuesRow thread={thread} />);
  await screen.findByRole("button", { name: /^#412 / });
  expect(screen.getByRole("button", { name: /^ORB-87 / })).toHaveTextContent("Linear · ORB-87");
  expect(screen.getByRole("button", { name: /^#391 / })).toHaveTextContent("Closed");
  expect(screen.getByRole("button", { name: /^#377 / })).toHaveTextContent("Closed as not planned");
  expect(screen.getAllByText("Issues")).toHaveLength(1);
  expect(screen.getByRole("button", { name: "Link an issue…" })).toBeVisible();
});

it("a long title is cut with an ellipsis in the line and kept whole in its tooltip", async () => {
  const long = "A very long title ".repeat(8).trim();
  setIssuesApi(createIssuesMock({ links: { "thread-1": [{ ...refOf("412"), title: long }] } }));
  render(<IssuesRow thread={thread} />);
  expect(await screen.findByText(long, { selector: ".iss-dtitle" })).toHaveAttribute("title", long);
});

it("Unlink is one write however fast the clicks, and the line goes", async () => {
  const mock = createIssuesMock({ delayMs: 20 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} />);
  await user.dblClick(await screen.findByRole("button", { name: "Unlink #412" }));
  await screen.findByText("None linked");
  expect(mock.writes).toEqual(["unlink:thread-1:412"]);
});

it("pressing a line opens the issue beside the thread as a tab", async () => {
  setIssuesApi(createIssuesMock());
  const closed = vi.fn();
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} onOpened={closed} />);
  await user.click(await screen.findByRole("button", { name: "#412 Login sends people back to /home" }));
  const state = useUi.getState();
  expect(state.panelOpen).toBe(true);
  expect(state.panelTab).toBe(fileTabId({ kind: "issue", tracker: "github", key: "412" }));
  expect(parseFileTab(state.panelTab as string)).toEqual({ kind: "issue", tracker: "github", key: "412" });
  expect(tabOf(state.panelTab!).label).toBe("#412");
  expect(closed).toHaveBeenCalledOnce();
});

// ---- Link an issue ----

const openPicker = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.click(await screen.findByRole("button", { name: "Link an issue…" }));
  return screen.findByRole("dialog", { name: "Link an issue" });
};

it("choosing a row writes nothing, Link is one write however fast the clicks, and the thread gets the line", async () => {
  const mock = createIssuesMock({ links: {}, delayMs: 20 });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} />);
  const dialog = await openPicker(user);
  const options = await within(dialog).findAllByRole("option");
  expect(options.map((option) => option.textContent).join("|")).not.toContain("#391");
  expect(within(dialog).getByRole("button", { name: "Link" })).toBeDisabled();
  expect(options[0]).toHaveTextContent("GitHub · orbyn-dev/orbyn-engineering");
  await user.click(options[0]!);
  expect(mock.writes).toEqual([]);
  const link = within(dialog).getByRole("button", { name: "Link" });
  await user.dblClick(link);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toHaveLength(1);
  expect(mock.writes[0]).toMatch(/^link:thread-1:/);
  expect(await screen.findByRole("button", { name: /Unlink/ })).toBeVisible();
});

it("searches by words and says when nothing matches", async () => {
  setIssuesApi(createIssuesMock({ links: {} }));
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} />);
  const dialog = await openPicker(user);
  await within(dialog).findAllByRole("option");
  await user.type(within(dialog).getByRole("combobox", { name: "Search issues" }), "icon picker");
  await waitFor(() => expect(within(dialog).getAllByRole("option")).toHaveLength(2));
  await user.clear(within(dialog).getByRole("combobox"));
  await user.type(within(dialog).getByRole("combobox"), "zzzz");
  expect(await within(dialog).findByText("No issue matches.")).toBeVisible();
});

it("with no tracker connected the one action is Open Settings, at Source control, and nothing is searched", async () => {
  const mock = createIssuesMock({ links: {}, trackers: [{ tracker: "github", connected: false }] });
  const finding = vi.spyOn(mock, "find");
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} />);
  const dialog = await openPicker(user);
  expect(within(dialog).getByText("Connect a tracker")).toBeVisible();
  expect(within(dialog).getByText(/Orbyn needs one tracker to find issues/)).toBeVisible();
  expect(within(dialog).queryByRole("button", { name: "Link" })).toBeNull();
  await user.click(within(dialog).getByRole("button", { name: "Open Settings" }));
  expect(window.location.hash).toBe("#/settings/source-control");
  expect(finding).not.toHaveBeenCalled();
  expect(mock.writes).toEqual([]);
});

it("when the trackers cannot be read it says what failed and that nothing changed, and Retry asks again", async () => {
  const mock = createIssuesMock({ links: {} });
  mock.failNext("find");
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} />);
  const dialog = await openPicker(user);
  const alert = await within(dialog).findByRole("alert");
  expect(alert).toHaveTextContent("Can’t read your trackers");
  expect(alert).toHaveTextContent("GitHub, Forgejo and Linear did not answer. Nothing is changed. Retry asks again.");
  expect(within(dialog).getByRole("button", { name: "Link" })).toBeDisabled();
  await user.click(within(dialog).getByRole("button", { name: "Retry" }));
  expect((await within(dialog).findAllByRole("option")).length).toBeGreaterThan(0);
  expect(mock.writes).toEqual([]);
});

it("a refused link says so, links nothing and leaves the dialog open", async () => {
  const mock = createIssuesMock({ links: {} });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssuesRow thread={thread} />);
  const dialog = await openPicker(user);
  await user.click((await within(dialog).findAllByRole("option"))[0]!);
  vi.spyOn(mock, "link").mockRejectedValueOnce(new Error("The tracker said no."));
  await user.click(within(dialog).getByRole("button", { name: "Link" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Couldn’t link it. Nothing changed. The tracker said no.");
  expect(screen.getByRole("dialog")).toBeVisible();
});

// ---- the issue as a panel tab ----

it("reads the issue into the panel: key, title, state, labels, body and activity, with a link out", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuePanel tracker="github" issueKey="412" />);
  expect(await screen.findByRole("heading", { name: "Login sends people back to /home" })).toBeVisible();
  expect(screen.getByText("#412 · GitHub · orbyn-dev/orbyn-engineering")).toBeVisible();
  expect(screen.getByText("auth")).toBeVisible();
  expect(screen.getByText("Reproduced. The redirect reads", { exact: false })).toBeVisible();
  expect(screen.getByRole("link", { name: "Open on GitHub" })).toHaveAttribute("href", "https://github.com/orbyn-dev/orbyn-engineering/issues/412");
  expect(screen.getByRole("link", { name: "Open as a page" })).toHaveAttribute("href", "#/issues/github/412");
  expect(screen.queryByRole("textbox")).toBeNull();
});

it("says a missing issue is missing, and retries a failed read", async () => {
  const mock = createIssuesMock();
  setIssuesApi(mock);
  const { unmount } = render(<IssuePanel tracker="github" issueKey="9999" />);
  expect(await screen.findByText("No issue #9999 on GitHub.")).toBeVisible();
  unmount();
  mock.failNext("get");
  const user = userEvent.setup();
  render(<IssuePanel tracker="github" issueKey="412" />);
  await user.click(await screen.findByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("heading", { name: "Login sends people back to /home" })).toBeVisible();
});

it("an issue tab is kept in the panel's order and closes like a file tab", () => {
  const id = fileTabId({ kind: "issue", tracker: "linear", key: "ORB-87" });
  useUi.getState().openFileTab({ kind: "issue", tracker: "linear", key: "ORB-87" });
  expect(useUi.getState().panelTabOrder).toContain(id);
  expect(tabOf(id).label).toBe("ORB-87");
  useUi.getState().closePanelTab(id);
  expect(useUi.getState().panelTabOrder).not.toContain(id);
});
