import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { App } from "../../../app/App";
import { dataChanged } from "../../../api/shell";
import { startDataRefresh } from "../../../lib/dataRefresh";
import { fileTabId, useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { folderKey, projectKey, worktreeKey } from "./model";
import { levelKey, useExplorer } from "./store";

const treeOf = () => screen.getByRole("tree", { name: "Projects, worktrees and files" });
/** A row is named by what it holds: its name, then a change, a status or a button. A string matches its start. */
const named = (name: string | RegExp) => (typeof name === "string" ? new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}`) : name);
const row = (name: string | RegExp) => within(treeOf()).getByRole("treeitem", { name: named(name) });
const maybeRow = (name: string | RegExp) => within(treeOf()).queryByRole("treeitem", { name: named(name) });
const findRow = (name: string | RegExp) => screen.findByRole("treeitem", { name: named(name) });
/** The explorer reads the service made, as paths: worktree lists, folders and files. */
const reads = () => (fetch as unknown as ReturnType<typeof vi.fn>).mock.calls.map((call) => String(call[0])).filter((url) => url.includes("/worktrees"));
const folderReads = (path: string) => reads().filter((url) => url.includes(`/tree?path=${encodeURIComponent(path)}`));

beforeEach(() => {
  useExplorer.getState().reset();
  installMockFetch();
  useUi.setState({ projectFolders: true, panelOpen: false, panelTab: null, leftBarView: "files", explorerOpen: {}, sidebarWidth: 224 });
  useUi.getState().setPrefs({ welcomeDone: true });
  window.location.hash = "#/threads/t-done";
});

afterEach(() => vi.restoreAllMocks());

it("opens on the thread's own project and worktree, and reads a folder only when it is unfolded", async () => {
  const user = userEvent.setup();
  render(<App />);
  // The open thread's worktree (main) is unfolded and marked; its root is read, the folders in it are not.
  expect(await screen.findByText("README.md")).toBeInTheDocument();
  expect(row("main")).toHaveAttribute("aria-expanded", "true");
  expect(within(row("main")).getByRole("img", { name: "Current worktree" })).toBeInTheDocument();
  expect(folderReads("")).toHaveLength(1);
  expect(folderReads("src")).toHaveLength(0);
  // Other worktrees and other projects start folded, and nothing of theirs is read.
  expect(row("approval-timeout")).toHaveAttribute("aria-expanded", "false");
  expect(row("marketing-site")).toHaveAttribute("aria-expanded", "false");
  expect(reads().some((url) => url.includes("/api/projects/p-site/"))).toBe(false);

  await user.click(row("src"));
  expect(await screen.findByText("main.rs")).toBeInTheDocument();
  expect(folderReads("src")).toHaveLength(1);
  // Folding and unfolding again does not read it again: it is held.
  await user.click(row("src"));
  await user.click(row("src"));
  expect(folderReads("src")).toHaveLength(1);
});

it("remembers what a person unfolded", async () => {
  const user = userEvent.setup();
  const first = render(<App />);
  await user.click(await findRow("marketing-site"));
  await waitFor(() => expect(within(treeOf()).getAllByRole("treeitem", { name: /^main/ })).toHaveLength(2));
  await user.click(within(treeOf()).getAllByRole("treeitem", { name: /^main/ }).at(-1)!);
  expect(await screen.findByText("index.html")).toBeInTheDocument();
  first.unmount();
  useExplorer.getState().reset();
  render(<App />);
  expect(await screen.findByText("index.html")).toBeInTheDocument();
  expect(row("marketing-site")).toHaveAttribute("aria-expanded", "true");
});

it("draws a change as a letter at the right of a file and none on a folder", async () => {
  render(<App />);
  await screen.findByText("README.md");
  expect(within(row("README.md")).getByText("M")).toHaveAccessibleName("Modified");
  expect(within(row("src")).queryByText("M")).toBeNull();
  expect(within(row("Cargo.toml")).queryByText(/^[MADRU!]$/)).toBeNull();
});

it("lists the threads of a project under it, and a click opens the thread", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("README.md");
  expect(row("Threads")).toHaveAttribute("aria-expanded", "true");
  await user.click(row("Upgrade tokio and fix the fallout"));
  expect(window.location.hash).toBe("#/threads/t-failed");
});

it("opens a file as a tab in the side panel and marks its row", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow(/^README\.md/));
  const id = fileTabId({ kind: "explorer", projectId: "p-app", worktreeId: "p-app-main", path: "README.md", project: "orbyn-engineering-app", worktree: "main" });
  expect(useUi.getState()).toMatchObject({ panelOpen: true, panelTab: id });
  expect(await screen.findByRole("tab", { name: "README.md" })).toHaveAttribute("aria-selected", "true");
  await waitFor(() => expect(row(/^README\.md/)).toHaveAttribute("aria-selected", "true"));
  // The thread runs in this worktree, so it is the thread's own file, read by the thread: no read-only banner.
  expect(await screen.findByText(/Mock file contents/)).toBeInTheDocument();
  expect(screen.queryByText(/Read-only/)).toBeNull();
});

it("reads another worktree's file read-only, with a way to start a thread there", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("approval-timeout"));
  // The worktree says Read-only while it is unfolded.
  await waitFor(() => expect(within(row("approval-timeout")).getByText("Read-only")).toBeInTheDocument());
  // Its own copy of the files is below it, after main's.
  await waitFor(() => expect(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ })).toHaveLength(2));
  await user.click(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ }).at(-1)!);
  const banner = await screen.findByText("Read-only · not this thread’s worktree");
  expect(banner).toBeInTheDocument();
  expect(await screen.findByText("// approval-timeout")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Start a thread here" }));
  expect(window.location.hash).toBe("#/new/p-app");
});

it("shows a worktree whose folder is gone as one error row with Try again, and keeps the rows above usable", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("feat/gone"));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Couldn’t read this folder.");
  expect(within(alert).getByRole("button", { name: "Try again" })).toBeInTheDocument();
  expect(row("main")).toBeInTheDocument();
  const before = folderReads("").length;
  await user.click(within(alert).getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(folderReads("").length).toBeGreaterThan(before));
});

it("reads a locked worktree and marks it Read-only", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("feat/locked"));
  await waitFor(() => expect(within(row("feat/locked")).getByText("Read-only")).toBeInTheDocument());
  // Locked is for writing only: its files read, and a file opens read-only.
  await waitFor(() => expect(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ })).toHaveLength(2));
  await user.click(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ }).at(-1)!);
  expect(await screen.findByText("// feat/locked")).toBeInTheDocument();
  // A locked worktree is not another thread's: the banner says Read-only and offers no thread there.
  expect(within(screen.getByRole("tabpanel")).getByText("Read-only")).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Start a thread here" })).toBeNull();
});

it("says a folder with nothing in it is empty, not an error", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("empty"));
  expect(await screen.findByText("This folder is empty.")).toBeInTheDocument();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("shows a large folder 200 children at a time", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("src"));
  await user.click(await findRow("generated"));
  expect(await screen.findByText("Show 60 more")).toBeInTheDocument();
  expect(maybeRow(/file-199/)).not.toBeNull();
  expect(maybeRow(/file-200/)).toBeNull();
  await user.click(screen.getByText("Show 60 more"));
  expect(await findRow(/file-259/)).toBeInTheDocument();
  expect(screen.queryByText(/Show \d+ more/)).toBeNull();
});

it("draws only the rows on screen in a long list", async () => {
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(300);
  const dir = (path: string) => folderKey("p-app", "p-app-main", path);
  useUi.setState({ explorerOpen: { [projectKey("p-app")]: true, [worktreeKey("p-app", "p-app-main")]: true, [dir("src")]: true, [dir("src/generated")]: true } });
  render(<App />);
  await waitFor(() => expect(useExplorer.getState().levels[levelKey("p-app", "p-app-main", "src/generated")]?.data?.entries).toHaveLength(260));
  // Over 250 rows are held; the ones on screen and a few around them are drawn.
  const drawn = () => within(treeOf()).getAllByRole("treeitem");
  expect(drawn().length).toBeLessThan(40);
  expect(within(treeOf()).queryByRole("treeitem", { name: /file-/ })).toBeNull();
  // Scrolling draws what comes into view and lets go of the rest.
  const scroller = treeOf().parentElement!;
  scroller.scrollTop = 25 * 100;
  fireEvent.scroll(scroller);
  await waitFor(() => expect(within(treeOf()).getAllByRole("treeitem", { name: /file-/ }).length).toBeGreaterThan(5));
  expect(drawn().length).toBeLessThan(40);
});

it("moves with the arrow keys, opens and closes with right and left, and opens a file with Enter", async () => {
  const user = userEvent.setup();
  render(<App />);
  await screen.findByText("README.md");
  // The tree is one tab stop.
  expect(within(treeOf()).getAllByRole("treeitem").filter((item) => item.tabIndex === 0)).toHaveLength(1);
  row("docs").focus();
  await user.keyboard("{ArrowRight}");
  expect(row("docs")).toHaveAttribute("aria-expanded", "true");
  await user.keyboard("{ArrowRight}");
  expect(await findRow(/^guide\.md/)).toHaveFocus();
  await user.keyboard("{ArrowLeft}");
  expect(row("docs")).toHaveFocus();
  await user.keyboard("{ArrowLeft}");
  expect(row("docs")).toHaveAttribute("aria-expanded", "false");
  await user.keyboard("{ArrowDown}{ArrowDown}");
  expect(within(treeOf()).getByRole("treeitem", { name: /^empty/ })).not.toBeNull();
  await user.keyboard("{Home}");
  expect(within(treeOf()).getAllByRole("treeitem")[0]).toHaveFocus();
  await user.keyboard("{End}");
  expect(within(treeOf()).getAllByRole("treeitem").at(-1)).toHaveFocus();
  row(/^README\.md/).focus();
  await user.keyboard("{Enter}");
  expect(await screen.findByRole("tab", { name: "README.md", selected: true })).toBeInTheDocument();
});

it("lets a file of the thread's own worktree be dragged into its message, and no other", async () => {
  const user = userEvent.setup();
  render(<App />);
  const own = await findRow(/^README\.md/);
  const setData = vi.fn();
  fireEvent.dragStart(own, { dataTransfer: { setData } });
  expect(setData).toHaveBeenCalledWith(expect.stringContaining("file"), "README.md");
  await user.click(await findRow("approval-timeout"));
  await waitFor(() => expect(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ })).toHaveLength(2));
  expect(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ }).at(-1)).not.toHaveAttribute("draggable");
});

it("offers Open and the path copies on a file, and no worktree removal", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.pointer({ keys: "[MouseRight]", target: await findRow(/^README\.md/) });
  const menu = await screen.findByRole("menu", { name: "File actions" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(
    expect.arrayContaining(["Open↵", "Add to this thread@", "Copy path", "Copy relative path"]),
  );
  await user.keyboard("{Escape}");
  await user.pointer({ keys: "[MouseRight]", target: row("main") });
  const worktree = await screen.findByRole("menu", { name: "Worktree actions" });
  expect(within(worktree).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["New thread here", "Copy path"]);
});

it("reads a project's files again, quietly, when the service says they changed", async () => {
  const stop = startDataRefresh();
  render(<App />);
  await screen.findByText("README.md");
  const before = folderReads("").length;
  act(() => dataChanged("explorer:p-app"));
  await waitFor(() => expect(folderReads("").length).toBe(before + 1));
  // It was quiet: the tree never went back to a loading state.
  expect(screen.getByText("README.md")).toBeInTheDocument();
  // Another project's files changed: nothing of this one is read.
  const now = reads().length;
  act(() => dataChanged("explorer:p-site"));
  await new Promise((done) => setTimeout(done, 400));
  expect(reads().length).toBe(now);
  stop();
});

it("switches between Threads and Files, remembers the choice, and shortens the switch to icons at the minimum width", async () => {
  const user = userEvent.setup();
  useUi.setState({ leftBarView: "threads" });
  render(<App />);
  const files = await screen.findByRole("tab", { name: "Files" });
  expect(screen.getByRole("tab", { name: "Threads" })).toHaveAttribute("aria-selected", "true");
  await user.click(files);
  expect(await screen.findByText("README.md")).toBeInTheDocument();
  expect(JSON.parse(localStorage.getItem("orbyn.prefs.v1") ?? "{}").leftBarView).toBe("files");
  await user.click(screen.getByRole("tab", { name: "Threads" }));
  expect(screen.queryByRole("tree", { name: "Projects, worktrees and files" })).toBeNull();
  act(() => useUi.setState({ sidebarWidth: 180 }));
  expect(screen.getByRole("tab", { name: "Files" })).toHaveTextContent("");
});
