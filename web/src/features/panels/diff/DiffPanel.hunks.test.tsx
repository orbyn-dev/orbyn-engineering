import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, ApiFailure } from "../../../api/client";
import { pullsApi } from "../../../api/pulls";
import type { FileDiff } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import * as panelsApi from "../api";
import { DiffPanel } from "./DiffPanel";

const HEADER = "diff --git a/auth/redirect.ts b/auth/redirect.ts\nindex 1111111..2222222 100644\n--- a/auth/redirect.ts\n+++ b/auth/redirect.ts\n";
const HUNK = "@@ -14,3 +14,4 @@ after(req)\n export function after(req) {\n-  return \"/home\"\n+  const next = req.query.next\n+  return inside(next) ? next : \"/\"\n }\n";
const REDIRECT: FileDiff = { path: "auth/redirect.ts", status: "modified", additions: 2, deletions: 1, patch: HEADER + HUNK };
const DISK = `${"// filler\n".repeat(13)}export function after(req) {\n  const next = req.query.next\n  return inside(next) ? next : "/"\n}\n`;
const BEFORE = `${"// filler\n".repeat(13)}export function after(req) {\n  return "/home"\n}\n`;

function setup(options: { width?: number; files?: FileDiff[] } = {}) {
  const diff = vi.spyOn(api, "diff").mockResolvedValue(options.files ?? [REDIRECT]);
  vi.spyOn(api, "checkpoints").mockResolvedValue([]);
  vi.spyOn(api, "commits").mockResolvedValue([]);
  vi.spyOn(api, "jjStatus").mockResolvedValue(null);
  vi.spyOn(api, "branches").mockResolvedValue([{ name: "fix/login", current: true, remote: false }]);
  vi.spyOn(pullsApi, "linked").mockResolvedValue([]);
  vi.spyOn(api, "resolveThreadPath").mockResolvedValue({ cwd: "/repo", checkout_path: "auth/redirect.ts", repository_path: "auth/redirect.ts" });
  const read = vi.spyOn(panelsApi, "readThreadFile").mockResolvedValue({ path: "auth/redirect.ts", content: DISK, revision: "r1", size: DISK.length, binary: false, truncated: false });
  const save = vi.spyOn(panelsApi, "saveThreadFile").mockResolvedValue({ revision: "r2" } as never);
  useWorkspace.setState({ threads: { t: { id: "t", cwd: "/repo", active_run: null, worktree: { path: "/repo", branch: "fix/login", base: "main" } } as never } });
  if (options.width) {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const width = this.classList.contains("x2-pbody") ? options.width! : 0;
      return { width, height: 0, top: 0, left: 0, right: width, bottom: 0, x: 0, y: 0, toJSON: () => ({}) };
    });
  }
  return { diff, read, save };
}

const open = async () => {
  render(<DiffPanel threadId="t" />);
  await screen.findByRole("button", { name: "Scope: Working tree" });
};

beforeEach(() => {
  Element.prototype.scrollIntoView = vi.fn();
  HTMLDialogElement.prototype.showModal ??= function showModal(this: HTMLDialogElement) { this.setAttribute("open", ""); };
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.useRealTimers();
  useComposerChips.setState({ chips: {}, sent: {} });
});

// hunk-actions
it("a hunk's header offers Revert hunk and its added lines a pencil; a removed line has none", async () => {
  setup();
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  expect(within(patch).getByRole("button", { name: "Revert hunk" })).toBeInTheDocument();
  expect(within(patch).getAllByRole("button", { name: "Edit this line" })).toHaveLength(4);
  const removed = patch.querySelector(".l.del")!;
  expect(within(removed as HTMLElement).queryByRole("button", { name: "Edit this line" })).toBeNull();
});

it("offers neither outside the Working tree scope", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Scope: Working tree" }));
  await user.click(await screen.findByRole("menuitem", { name: "Thread" }));
  await screen.findByRole("group", { name: "Patch" });
  expect(screen.queryByRole("button", { name: "Revert hunk" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Edit this line" })).toBeNull();
});

// hunk-revert-confirm, hunk-reverted
it("asks once in words that name the lines, with Cancel the default, and Esc cancels", async () => {
  const user = userEvent.setup();
  const { save } = setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Revert hunk" }));
  const dialog = await screen.findByRole("dialog", { name: "Revert this hunk?", hidden: true });
  expect(dialog).toHaveTextContent("Lines 15 to 16 of auth/redirect.ts go back to the last commit. You can undo it for 10 seconds afterwards; after that the thread’s checkpoints can bring the lines back.");
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  // Esc reaches a modal as the native cancel event.
  fireEvent(dialog, new Event("cancel", { cancelable: true }));
  await waitFor(() => expect(screen.queryByRole("dialog", { hidden: true })).toBeNull());
  expect(save).not.toHaveBeenCalled();
});

it("reverting writes the hunk back through the file route at the revision read, then offers Undo for 10 seconds", async () => {
  const user = userEvent.setup();
  const { save, diff } = setup();
  await open();
  await user.click(screen.getByRole("button", { name: "Revert hunk" }));
  await user.click(within(await screen.findByRole("dialog", { hidden: true })).getByRole("button", { name: "Revert hunk" }));
  await waitFor(() => expect(save).toHaveBeenCalledWith("t", { path: "auth/redirect.ts", content: BEFORE, base_revision: "r1" }));
  const toast = await screen.findByRole("status");
  expect(toast).toHaveTextContent("Hunk reverted in auth/redirect.ts.");
  expect(toast).toHaveTextContent("Undo · 10 s");
  await waitFor(() => expect(diff.mock.calls.length).toBeGreaterThan(1));
  await user.click(within(toast).getByRole("button", { name: /^Undo/ }));
  await waitFor(() => expect(save).toHaveBeenLastCalledWith("t", { path: "auth/redirect.ts", content: DISK, base_revision: "r2" }));
  expect(screen.queryByRole("status")).toBeNull();
});

it("the undo message counts down and goes away after 10 seconds,", async () => {
  setup();
  await open();
  vi.useFakeTimers({ shouldAdvanceTime: true });
  const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
  await user.click(screen.getByRole("button", { name: "Revert hunk" }));
  await user.click(within(await screen.findByRole("dialog", { hidden: true })).getByRole("button", { name: "Revert hunk" }));
  const toast = await screen.findByRole("status");
  for (let tick = 0; tick < 3; tick += 1) await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(toast).toHaveTextContent("Undo · 7 s");
  for (let tick = 0; tick < 8; tick += 1) await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(screen.queryByRole("status")).toBeNull();
});

// hunk-revert-error
it("says nothing was changed when the file moved on, and Refresh diff reads the diff again", async () => {
  const user = userEvent.setup();
  const { save, read, diff } = setup();
  read.mockResolvedValue({ path: "auth/redirect.ts", content: DISK.replace("req.query.next", "other"), revision: "r1", size: 1, binary: false, truncated: false });
  await open();
  await user.click(screen.getByRole("button", { name: "Revert hunk" }));
  await user.click(within(await screen.findByRole("dialog", { hidden: true })).getByRole("button", { name: "Revert hunk" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Couldn’t revert the hunk. The file has changed since this diff was read. Nothing was changed.");
  expect(save).not.toHaveBeenCalled();
  const reads = diff.mock.calls.length;
  await user.click(within(alert).getByRole("button", { name: "Refresh diff" }));
  await waitFor(() => expect(diff.mock.calls.length).toBeGreaterThan(reads));
  expect(screen.queryByText(/Couldn’t revert/)).toBeNull();
});

it("a write the service refuses as stale reads the same", async () => {
  const user = userEvent.setup();
  const { save } = setup();
  save.mockRejectedValue(new ApiFailure("file_conflict", "changed", 409));
  await open();
  await user.click(screen.getByRole("button", { name: "Revert hunk" }));
  await user.click(within(await screen.findByRole("dialog", { hidden: true })).getByRole("button", { name: "Revert hunk" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The file has changed since this diff was read. Nothing was changed.");
});

// hunk-edit, hunk-edit-error
it("the pencil turns the line into a field; ⌘↵ writes only that line, Esc cancels", async () => {
  const user = userEvent.setup();
  const { save } = setup();
  await open();
  const pens = screen.getAllByRole("button", { name: "Edit this line" });
  await user.click(pens[2]!);
  const field = screen.getByRole("textbox", { name: "Edit line 16" });
  expect(field).toHaveValue("  return inside(next) ? next : \"/\"");
  expect(screen.getByText(/Esc cancels · ⌘↵ saves to the file/)).toBeInTheDocument();
  await user.keyboard("{Escape}");
  expect(screen.queryByRole("textbox", { name: "Edit line 16" })).toBeNull();
  await user.dblClick(within(screen.getByRole("group", { name: "Patch" })).getByText(/return inside/));
  const again = screen.getByRole("textbox", { name: "Edit line 16" });
  await user.clear(again);
  await user.type(again, "  return next");
  await user.keyboard("{Meta>}{Enter}{/Meta}");
  await waitFor(() => expect(save).toHaveBeenCalledWith("t", { path: "auth/redirect.ts", content: DISK.replace('  return inside(next) ? next : "/"', "  return next"), base_revision: "r1" }));
  await waitFor(() => expect(screen.queryByRole("textbox", { name: "Edit line 16" })).toBeNull());
});

it("Save line writes it too, and a file that changed keeps the typed text and offers Reload diff", async () => {
  const user = userEvent.setup();
  const { save, diff } = setup();
  save.mockRejectedValue(new ApiFailure("file_conflict", "changed", 409));
  await open();
  await user.click(screen.getAllByRole("button", { name: "Edit this line" })[2]!);
  await user.type(screen.getByRole("textbox", { name: "Edit line 16" }), "!");
  await user.click(screen.getByRole("button", { name: "Save line" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Couldn’t save the line. auth/redirect.ts changed on disk. Reload the diff, then edit again.");
  expect(screen.getByRole("textbox", { name: "Edit line 16" })).toHaveValue("  return inside(next) ? next : \"/\"!");
  const reads = diff.mock.calls.length;
  await user.click(within(alert).getByRole("button", { name: "Reload diff" }));
  await waitFor(() => expect(diff.mock.calls.length).toBeGreaterThan(reads));
});

// hunk-select, hunk-send-chip
it("a selection offers Comment, Send selection to engine and Copy; sending adds a chip and sends nothing", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  await user.pointer([
    { keys: "[MouseLeft>]", target: within(patch).getByRole("button", { name: "Comment on line 15" }) },
    { target: within(patch).getByRole("button", { name: "Comment on line 16" }) },
    { keys: "[/MouseLeft]", target: within(patch).getByRole("button", { name: "Comment on line 16" }) },
  ]);
  const bar = await screen.findByRole("toolbar", { name: "Selection" });
  expect(within(bar).getAllByRole("button").map((button) => button.textContent || button.getAttribute("aria-label"))).toEqual(["Comment", "Send selection to engine", "Copy"]);
  await user.click(within(bar).getByRole("button", { name: "Send selection to engine" }));
  expect(useComposerChips.getState().chips.t).toEqual([
    expect.objectContaining({ kind: "file", path: "auth/redirect.ts", line: 15, endLine: 16, fromDiff: true, excerpt: "  const next = req.query.next\n  return inside(next) ? next : \"/\"" }),
  ]);
  // The lines stay selected, and sending twice does not stack a second chip.
  expect(patch.querySelectorAll(".l.lsel")).toHaveLength(2);
  await user.click(within(bar).getByRole("button", { name: "Send selection to engine" }));
  expect(useComposerChips.getState().chips.t).toHaveLength(1);
});

it("Enter on a selection sends it to the engine", async () => {
  const user = userEvent.setup();
  setup();
  await open();
  const patch = screen.getByRole("group", { name: "Patch" });
  patch.focus();
  await user.keyboard("{ArrowDown}{ArrowDown}{ArrowDown}{ArrowDown}{Shift>}{ArrowDown}{/Shift}{Enter}");
  expect(useComposerChips.getState().chips.t?.[0]).toMatchObject({ kind: "file", line: 15, endLine: 16 });
});

// hunk-narrow
it("in a narrow panel the hunk's actions fold into one More menu: Revert hunk, Edit a line and Copy hunk", async () => {
  const user = userEvent.setup();
  setup({ width: 320 });
  await open();
  expect(screen.queryByRole("button", { name: "Revert hunk" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Hunk actions" }));
  const menu = await screen.findByRole("menu", { name: "Hunk actions" });
  expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Revert hunk", "Edit a line…", "Copy hunk"]);
  await user.click(within(menu).getByRole("menuitem", { name: "Edit a line…" }));
  expect(screen.getByRole("textbox", { name: "Edit line 15" })).toBeInTheDocument();
});

// chip jump
it("a chip made from a selection takes the person back to its file in Changes", async () => {
  const { ComposerChipShelf } = await import("../../thread/composer/ComposerChips");
  useWorkspace.setState({ threads: { t: { id: "t", cwd: "/repo" } as never } });
  const chip = { id: "c", kind: "file" as const, path: "auth/redirect.ts", line: 15, endLine: 16, fromDiff: true, excerpt: "x", language: "typescript" };
  render(<ComposerChipShelf threadId="t" chips={[chip]} onRemove={() => undefined} onCopy={() => undefined} />);
  const label = screen.getByRole("button", { name: "auth/redirect.ts:15–16" });
  label.click();
  expect(useUi.getState().diffReveal).toMatchObject({ path: "auth/redirect.ts", threadId: "t" });
});
