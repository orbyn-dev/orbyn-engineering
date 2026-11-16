import { ApiFailure, api } from "../../../api/client";
import { Explorer } from "./Explorer";
import { FileTab } from "../../panels/files/FileTab";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useWorkspace } from "../../../product/state/workspace";
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
  useUi.setState({ projectFolders: true, panelOpen: false, panelTab: null, panelTabOrder: ["diff", "browser", "activity", "side", "tools"], panelTabsClosed: ["diff", "browser", "activity", "side", "tools"], layouts: {}, leftBarView: "files", explorerOpen: {}, sidebarWidth: 224 });
  useComposerChips.setState({ chips: {} });
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
  const id = fileTabId({ kind: "explorer", projectId: "p-app", worktreeId: "p-app-main", path: "README.md" });
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
  const banner = await screen.findByText("Read-only · not this thread’s worktree.");
  expect(banner).toBeInTheDocument();
  expect(await screen.findByDisplayValue(/\/\/ approval-timeout/)).toBeInTheDocument();
  // The file is in the editor, which cannot be typed in; the header says so beside the worktree's name.
  expect(screen.getByRole("textbox", { name: "Read README.md" })).toHaveAttribute("readonly");
  expect(screen.getAllByText("Read-only").length).toBeGreaterThan(0);
  await user.click(screen.getByRole("button", { name: "Start a thread here" }));
  expect(window.location.hash).toBe("#/new/p-app");
});

it("shows a worktree whose folder is gone muted and Missing, with nothing to unfold, and its tab says so", async () => {
  const user = userEvent.setup();
  render(<App />);
  const gone = await findRow("feat/gone");
  expect(gone).toHaveAccessibleName("feat/gone, missing from disk");
  expect(gone).not.toHaveAttribute("aria-expanded");
  expect(within(gone).getByText("Missing")).toBeInTheDocument();
  await user.click(gone);
  expect(await screen.findByText("This worktree is missing from disk.")).toBeInTheDocument();
  expect(screen.getByText("Nothing to browse")).toBeInTheDocument();
  expect(within(screen.getByRole("tabpanel")).getByRole("button", { name: "Refresh" })).toBeInTheDocument();
  // No flow removes a worktree yet, so none is offered.
  expect(screen.queryByRole("button", { name: /Remove worktree/ })).toBeNull();
  expect(folderReads("")).toHaveLength(1);
});

it("shows a folder that cannot be read as one error row with Try again, and keeps the rows above usable", async () => {
  const user = userEvent.setup();
  const real = fetch;
  let failing = true;
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) =>
    failing && String(url).includes("tree?path=docs") ? new Response(JSON.stringify({ code: "forbidden", message: "Permission denied." }), { status: 403 }) : real(url, init)));
  render(<App />);
  await user.click(await findRow("docs"));
  const alert = await screen.findByRole("treeitem", { name: "Couldn’t read this folder." });
  expect(alert).toHaveTextContent("Couldn’t read this folder.");
  expect(within(alert).getByRole("button", { name: "Try again" })).toBeInTheDocument();
  expect(row("main")).toBeInTheDocument();
  failing = false;
  await user.click(within(alert).getByRole("button", { name: "Try again" }));
  expect(await screen.findByRole("treeitem", { name: /^guide\.md/ })).toBeInTheDocument();
});

it("reads a locked worktree and marks it Read-only", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("feat/locked"));
  await waitFor(() => expect(within(row("feat/locked")).getByText("Locked")).toBeInTheDocument());
  // Locked is for writing only: its files read, and a file opens read-only.
  await waitFor(() => expect(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ })).toHaveLength(2));
  await user.click(within(treeOf()).getAllByRole("treeitem", { name: /^README\.md/ }).at(-1)!);
  expect(await screen.findByText("// feat/locked")).toBeInTheDocument();
  // Its own banner, and no thread to start there.
  expect(screen.getByText("This worktree is locked. Files open read-only.")).toBeInTheDocument();
  expect(screen.queryByText(/not this thread’s worktree/)).toBeNull();
  expect(screen.queryByRole("button", { name: "Start a thread here" })).toBeNull();
});

it("says a folder with nothing in it is empty, not an error", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("empty"));
  expect(await screen.findByText("This folder is empty.")).toBeInTheDocument();
  expect(screen.queryByRole("treeitem", { name: "Couldn’t read this folder." })).toBeNull();
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

it("closes a folder the service cut with one quiet row naming what is shown of how many, once all of it is on screen", async () => {
  const user = userEvent.setup();
  const dir = (path: string) => folderKey("p-app", "p-app-main", path);
  useUi.setState({ explorerOpen: { [projectKey("p-app")]: true, [worktreeKey("p-app", "p-app-main")]: true, [dir("src")]: true, [dir("src/renders")]: true } });
  render(<App />);
  expect(await screen.findByText("Show 800 more")).toBeInTheDocument();
  // No count row while the list still pages: the paging row says what is left.
  expect(screen.queryByText(/of 3,482 shown/)).toBeNull();
  for (let page = 0; page < 4; page += 1) await user.click(screen.getByText(/Show \d+ more/));
  const capped = await screen.findByRole("treeitem", { name: "1,000 of 3,482 shown" });
  // Plain muted text: nothing to press, no bar or spinner, and it takes no place among the entries.
  expect(capped).toHaveAttribute("aria-disabled", "true");
  expect(capped).not.toHaveAttribute("aria-posinset");
  expect(within(capped).queryByRole("button")).toBeNull();
  expect(screen.queryByText(/Filter to narrow/)).toBeNull();
  // A folder the service did not cut has none.
  expect(screen.queryAllByText(/ shown$/)).toHaveLength(1);
});

it("opens a file with no thread open as a read-only tab beside the new-thread composer, with no thread's tabs", async () => {
  const user = userEvent.setup();
  window.location.hash = "#/";
  useUi.setState({ explorerOpen: { [projectKey("p-app")]: true, [worktreeKey("p-app", "p-app-main")]: true } });
  render(<App />);
  // No worktree is the thread's, and none says Read-only: there is no thread to be read-only for.
  const readme = await findRow("README.md");
  expect(within(treeOf()).queryByRole("img", { name: "Current worktree" })).toBeNull();
  expect(within(row("main")).queryByText("Read-only")).toBeNull();
  expect(screen.queryByRole("complementary", { name: "Files" })).toBeNull();
  await user.click(readme);
  const panel = await screen.findByRole("complementary", { name: "Files" });
  expect(await within(panel).findByText("Read-only · no thread is open")).toBeInTheDocument();
  expect(await within(panel).findByDisplayValue(/\/\/ main/)).toBeInTheDocument();
  // Only the file's own tab, closable; none of a thread's surfaces, and no composer in the panel.
  expect(within(panel).getAllByRole("tab")).toHaveLength(1);
  expect(within(panel).getByRole("tab", { name: "README.md" })).toHaveAttribute("aria-selected", "true");
  expect(within(panel).queryByRole("button", { name: "Open a tab" })).toBeNull();
  // The only text field is the file itself, read-only: no composer.
  expect(within(panel).getAllByRole("textbox").map((box) => box.getAttribute("aria-label"))).toEqual(["Read README.md"]);
  expect(window.location.hash).toBe("#/");
  // The way to a thread stays: it opens the new-thread composer for that worktree.
  await user.click(within(panel).getByRole("button", { name: "Start a thread here" }));
  expect(window.location.hash).toBe("#/new/p-app");
  // Closing the panel gives the page back.
  await user.click(within(await screen.findByRole("complementary", { name: "Files" })).getByRole("button", { name: "Close side panel" }));
  expect(screen.queryByRole("complementary", { name: "Files" })).toBeNull();
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

/** The thread runs in a subfolder of the worktree, as one in a subfolder project does: the service roots a worktree at the Git root. */
function setSubfolder() {
  act(() => {
    const threads = useWorkspace.getState().threads;
    const done = threads["t-done"]!;
    useWorkspace.setState({ threads: { ...threads, "t-done": { ...done, cwd: done.cwd.replace(/\/src$/, "") + "/src" } } });
  });
}
async function inSubfolder() {
  render(<App />);
  await screen.findByText("README.md");
  await userEvent.setup().click(await findRow("src"));
  await screen.findByText("main.rs");
  setSubfolder();
  // The service's refresh puts the thread back at the project's folder; keep it where this test needs it.
  const stop = useWorkspace.subscribe((state) => {
    if (state.threads["t-done"] && !state.threads["t-done"].cwd.endsWith("/src")) setSubfolder();
  });
  stops.push(stop);
}
const stops: (() => void)[] = [];
afterEach(() => stops.splice(0).forEach((stop) => stop()));

it("names a file as a thread in a subfolder does, and only offers the thread's actions for what is inside its folder", async () => {
  const user = userEvent.setup();
  await inSubfolder();
  const setData = vi.fn();
  // The service's own refresh puts the thread back at the project's folder, so each step says where the thread is first.
  setSubfolder();
  fireEvent.dragStart(await findRow(/^main\.rs/), { dataTransfer: { setData } });
  expect(setData).toHaveBeenCalledWith(expect.stringContaining("file"), "main.rs");
  // README.md is in the worktree but above the thread's folder: no drag, no thread actions.
  setSubfolder();
  expect(row(/^README\.md/)).not.toHaveAttribute("draggable");
  await user.pointer({ keys: "[MouseRight]", target: row(/^README\.md/) });
  const menu = await screen.findByRole("menu", { name: "File actions" });
  expect(within(menu).queryByRole("menuitem", { name: /Add to this thread/ })).toBeNull();
  await user.keyboard("{Escape}");
  setSubfolder();
  row(/^main\.rs/).focus();
  await user.keyboard("@");
  expect(useComposerChips.getState().chips["t-done"]?.[0]).toMatchObject({ kind: "file", path: "main.rs" });
});

it("opens a file below the thread's folder as the thread's own file, by the path the thread names", async () => {
  const thread = { id: "t", project_id: "p", cwd: "/repo/packages/web", worktree: null } as never;
  window.location.hash = "#/threads/t";
  useWorkspace.setState({ threads: { t: thread } });
  const wt = { id: "w", path: "/repo", name: "repo", branch: "main", detached: false, head: "a", is_main: true, read_only: false, read_only_reason: null, threads: [{ id: "t", title: "", running: false }], dirty: null, is_subfolder: false };
  useExplorer.setState({ worktrees: { p: { data: { project_id: "p", git: true, worktrees: [wt] }, loading: false, error: null } } });
  const urls: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (url: string, init?: RequestInit) => {
    urls.push(`${init?.method ?? "GET"} ${url}`);
    if (String(url).endsWith("/paths/resolve")) {
      const body = JSON.parse(String(init?.body)) as { path: string };
      return new Response(JSON.stringify({ cwd: "/repo/packages/web", checkout_path: body.path, repository_path: null }));
    }
    return new Response(JSON.stringify({ path: "links.md", revision: "r", content: "[move link](kept.txt)\n", size: 21, binary: false, truncated: false }));
  }));
  render(<FileTab id={fileTabId({ kind: "explorer", projectId: "p", worktreeId: "w", path: "packages/web/links.md" })} threadId="t" />);
  // The thread's own viewer: it can be edited, and its markdown links work.
  expect(await screen.findByRole("radio", { name: "Edit" })).toBeInTheDocument();
  expect(screen.queryByText(/Read-only/)).toBeNull();
  const resolved = urls.find((url) => url.endsWith("/paths/resolve"))!;
  expect(resolved).toContain("/api/threads/t/paths/resolve");
  expect(urls.some((url) => url.includes("/files/content?path=links.md"))).toBe(true);
  // A file above the thread's folder, in the same worktree, reads through the explorer.
  urls.length = 0;
  render(<FileTab id={fileTabId({ kind: "explorer", projectId: "p", worktreeId: "w", path: "README.md" })} threadId="t" />);
  await waitFor(() => expect(urls.some((url) => url.includes("/api/projects/p/worktrees/w/file?path=README.md"))).toBe(true));
});

it("keeps Alt+Enter, like the menu, to files the thread's own folder holds", async () => {
  const user = userEvent.setup();
  vi.spyOn(api, "editors").mockResolvedValue([{ id: "code", name: "VS Code", launch_style: "x", file_manager: false }]);
  const open = vi.spyOn(api, "openInEditor").mockResolvedValue(undefined as never);
  await inSubfolder();
  await waitFor(() => expect(api.editors).toHaveBeenCalled());
  setSubfolder();
  row(/^README\.md/).focus();
  await user.keyboard("{Alt>}{Enter}{/Alt}");
  expect(open).not.toHaveBeenCalled();
  setSubfolder();
  row(/^main\.rs/).focus();
  await user.keyboard("{Alt>}{Enter}{/Alt}");
  await waitFor(() => expect(open).toHaveBeenCalledWith("t-done", { path: "main.rs", editor: "code" }));
});

it("offers New thread here on the primary checkout, and on a linked worktree only when Orbyn made it", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.pointer({ keys: "[MouseRight]", target: await findRow("main") });
  const main = await screen.findByRole("menu", { name: "Worktree actions" });
  expect(within(main).getByRole("menuitem", { name: "New thread here" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  // approval-timeout belongs to a thread of ours; locked is not one Orbyn knows.
  await user.pointer({ keys: "[MouseRight]", target: row("approval-timeout") });
  expect(within(await screen.findByRole("menu", { name: "Worktree actions" })).getByRole("menuitem", { name: "New thread here" })).toBeInTheDocument();
  await user.keyboard("{Escape}");
  await user.pointer({ keys: "[MouseRight]", target: row("feat/locked") });
  expect(within(await screen.findByRole("menu", { name: "Worktree actions" })).queryByRole("menuitem", { name: "New thread here" })).toBeNull();
});

it("says a projects list that could not be read, instead of waiting for it", () => {
  useWorkspace.setState({ projects: { data: null, error: new ApiFailure("internal", "Nope.", 500), loading: false } });
  render(<Explorer route={{ name: "home" }} />);
  expect(screen.getByText("Couldn't load projects")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
});

it("gives each row its place among its siblings, and a thread one level below its Threads group", async () => {
  render(<App />);
  await screen.findByText("README.md");
  const group = row("Threads");
  expect(group).toHaveAttribute("aria-level", "2");
  expect(row("Upgrade tokio and fix the fallout")).toHaveAttribute("aria-level", "3");
  // Six entries in the worktree's root, folders first; the list may be long, so the place is stated.
  expect(row("src")).toHaveAttribute("aria-setsize", "6");
  expect(row("src")).toHaveAttribute("aria-posinset", "3");
  expect(row(/^README\.md/)).toHaveAttribute("aria-posinset", "5");
});

it("draws every change letter, a conflict in neutral ink and a deleted file struck through", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("src"));
  await user.click(await findRow("auth"));
  const letter = (name: RegExp, text: string) => expect(within(row(name)).getByText(text)).toBeInTheDocument();
  letter(/^token\.ts/, "!");
  letter(/^login\.ts/, "R");
  letter(/^legacy\.ts/, "D");
  letter(/^scratch\.md/, "U");
  expect(row(/^token\.ts/)).toHaveClass("xg-c");
  expect(row(/^legacy\.ts/)).toHaveClass("xg-d");
  expect(within(row(/^token\.ts/)).getByText("!")).toHaveClass("xg-bang");
});

it("lists a project that is not a Git repository as one folder with its files, no worktrees", async () => {
  const user = userEvent.setup();
  render(<App />);
  await user.click(await findRow("vendor-sdk"));
  expect(await findRow(/^index\.html/)).toBeInTheDocument();
  const items = within(treeOf()).getAllByRole("treeitem").map((item) => item.textContent);
  // After the p-new project row, its files sit directly below it: no Worktrees group, no branch.
  const at = items.findIndex((text) => text?.startsWith("vendor-sdk"));
  expect(items.slice(at + 1, at + 3).join()).toMatch(/index\.html/);
  expect(items.slice(at + 1).some((text) => text === "Worktrees")).toBe(false);
});
