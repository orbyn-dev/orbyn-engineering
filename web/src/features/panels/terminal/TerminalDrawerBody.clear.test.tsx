import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { TerminalInfo } from "../../../api/types";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { wire } from "../../../wire.test-support";
import type { CommandBlock } from "./blocks/blocksContract";
import { TerminalDrawerBody } from "./TerminalDrawerBody";

vi.mock("./TerminalPanel", () => ({ TerminalPanel: ({ terminalId }: { terminalId?: string }) => <div>Output of {terminalId ?? "main"}</div> }));

// The page's ability to send the person key decides whether Clear all blocks is offered at all, as it does for Cancel run
// (api/personKey.ts). The rest of the module stays real, so the request still carries the key a dev preview sends.
let canClear = true;
vi.mock("../../../api/personKey", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../../api/personKey")>()),
  canSendPersonKey: () => canClear,
}));

type User = ReturnType<typeof userEvent.setup>;

let mock: MockApi;

const terminal = (over: Partial<TerminalInfo>): TerminalInfo => ({
  id: "t2",
  thread_id: "th",
  title: "npm test",
  group: "g2",
  direction: null,
  alive: false,
  exit_code: 0,
  clients: 1,
  script: null,
  created_at: new Date(Date.now() - 62_000).toISOString(),
  ...over,
});
const startedAt = new Date(Date.now() - 5_000).toISOString();
const finished = (id: string, command: string): CommandBlock => ({
  id, command, started_at: startedAt, ended_at: new Date().toISOString(), exit_code: 0, cwd: null,
  output_bytes: 6, truncated: false, dropped_lines: 0, fullscreen: false,
});
const running = (id: string): CommandBlock => ({
  id, command: "npm run dev", started_at: startedAt, ended_at: null, exit_code: null, cwd: null,
  output_bytes: 6, truncated: false, dropped_lines: 0, fullscreen: false,
});
const outputs = (blocks: CommandBlock[]) => Object.fromEntries(blocks.map((block) => [block.id, "line\n"]));
/** The one DELETE each terminal's clear sends, in the order they went out. */
const clears = () => wire().filter((call) => call.method === "DELETE" && /\/blocks$/.test(call.path)).map((call) => call.path);
const blockReads = () => wire().filter((call) => call.method === "GET" && /\/blocks$/.test(call.path));
const toasts = () => useUi.getState().toasts.map((toast) => toast.text);

beforeEach(() => {
  canClear = true;
  mock = installMockFetch();
  useComposerChips.setState({ chips: {} });
  useUi.setState({ terminalOpen: true, toasts: [] });
  useConnection.setState({ offline: false, serviceDown: false });
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** Shows npm test, with these blocks, and nothing else in the way. */
async function show(blocks: CommandBlock[], terminals: TerminalInfo[] = [terminal({})]) {
  mock.terminals.seed("th", terminals);
  mock.terminals.setBlocks("th", "t2", blocks, outputs(blocks));
  const user = userEvent.setup();
  render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
  await user.click(await screen.findByRole("tab", { name: /npm test/ }));
  return user;
}

const openTabMenu = (user: User, name: RegExp = /npm test/) =>
  user.pointer({ keys: "[MouseRight]", target: screen.getByRole("tab", { name }) });

/** Presses the menu item and then the dialog's red button. */
async function clearFromMenu(user: User) {
  await user.click(screen.getByRole("menuitem", { name: "Clear all blocks…" }));
  const dialog = await screen.findByRole("dialog", { name: "Clear all blocks?" });
  await user.click(within(dialog).getByRole("button", { name: "Clear all blocks" }));
}

describe("Clear all blocks", () => {
  it("offers Clear all blocks… after Copy all output, and one press only asks", async () => {
    const user = await show([finished("b1", "npm test")]);
    await openTabMenu(user);
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Split right",
      "Split down",
      "Clear output",
      "Copy all output",
      "Clear all blocks…",
      "Watch only",
    ]);
    await user.click(screen.getByRole("menuitem", { name: "Clear all blocks…" }));
    expect(await screen.findByRole("dialog", { name: "Clear all blocks?" })).toBeVisible();
    // The press asks; it never clears on itself.
    expect(clears()).toEqual([]);
  });

  it("asks in the frame's words, with Cancel focused, and Esc closes it without a write", async () => {
    const user = await show([finished("b1", "npm test")]);
    await openTabMenu(user);
    await user.click(screen.getByRole("menuitem", { name: "Clear all blocks…" }));
    const dialog = await screen.findByRole("dialog", { name: "Clear all blocks?" });
    expect(within(dialog).getByText("The finished commands and their output are dropped from this terminal. A command that is still running stays, and its output keeps growing. The terminal and its scrollback stay.")).toBeInTheDocument();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(within(dialog).getByRole("button", { name: "Clear all blocks" })).toBeEnabled();
    fireEvent(dialog, new Event("cancel", { cancelable: true }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(clears()).toEqual([]);
    expect(toasts()).toEqual([]);
  });

  it("sends one clear for the terminal after the confirmation, then reads its blocks again", async () => {
    const user = await show([finished("b1", "make build"), running("r1")]);
    await openTabMenu(user);
    const readsBefore = blockReads().length;
    await clearFromMenu(user);
    await waitFor(() => expect(clears()).toEqual(["/api/threads/th/terminals/t2/blocks"]));
    // The route is Person-class, so the write carries the person key.
    const sent = vi.mocked(fetch).mock.calls.find(([url, init]) => init?.method === "DELETE" && String(url) === "/api/threads/th/terminals/t2/blocks");
    expect(new Headers(sent?.[1]?.headers).get("x-orbyn-person")).toBe("mock-person-key");
    // The page asks the list again for itself, not only on its own clock.
    await waitFor(() => expect(blockReads().length).toBeGreaterThan(readsBefore));
  });

  it("says the counts in one toast, keeps the running command, and drops the finished ones", async () => {
    const user = await show([...Array.from({ length: 5 }, (_, index) => finished(`f${index}`, `make ${index}`)), running("r1")]);
    await openTabMenu(user);
    await clearFromMenu(user);
    await waitFor(() => expect(toasts()).toEqual(["Cleared 5 blocks. 1 running command kept."]));
    await waitFor(() => expect(screen.queryByRole("group", { name: "Command make 0" })).toBeNull());
    expect(await screen.findByRole("group", { name: "Command npm run dev" })).toBeInTheDocument();
  });

  it("falls back to the terminal alone when nothing is left, and keeps the item for the next command", async () => {
    const user = await show([finished("b1", "npm test")]);
    await openTabMenu(user);
    await clearFromMenu(user);
    await waitFor(() => expect(toasts()).toEqual(["Cleared 1 block."]));
    await waitFor(() => expect(screen.queryByRole("group", { name: "Command npm test" })).toBeNull());
    // The production client draws no empty card for an empty list: the terminal alone stays.
    expect(document.querySelector(".cb-list")).toBeNull();
    expect(screen.getByText("Output of t2")).toBeInTheDocument();
    await openTabMenu(user);
    expect(screen.getByRole("menuitem", { name: "Clear all blocks…" })).toBeEnabled();
  });

  it("says there were no blocks to clear when nothing had finished", async () => {
    const user = await show([]);
    await openTabMenu(user);
    await clearFromMenu(user);
    await waitFor(() => expect(toasts()).toEqual(["There were no blocks to clear."]));
    expect(clears()).toEqual(["/api/threads/th/terminals/t2/blocks"]);
  });

  it("clears each terminal of a split tab with one write each", async () => {
    mock.terminals.seed("th", [terminal({}), terminal({ id: "t3", title: "vite", direction: "horizontal" })]);
    mock.terminals.setBlocks("th", "t2", [finished("b1", "npm test")], { b1: "ok\n" });
    mock.terminals.setBlocks("th", "t3", [finished("b2", "vite build")], { b2: "ok\n" });
    const user = userEvent.setup();
    render(<TerminalDrawerBody threadId="th" onClose={() => undefined} />);
    await user.click(await screen.findByRole("tab", { name: /npm test \(2\)/ }));
    await openTabMenu(user, /npm test \(2\)/);
    await clearFromMenu(user);
    await waitFor(() => expect(clears().sort()).toEqual(["/api/threads/th/terminals/t2/blocks", "/api/threads/th/terminals/t3/blocks"]));
    await waitFor(() => expect(toasts()).toEqual(["Cleared 2 blocks."]));
  });

  it("offers no Clear all blocks where the page cannot send the person key", async () => {
    canClear = false;
    const user = await show([finished("b1", "npm test")]);
    await openTabMenu(user);
    expect(screen.queryByRole("menuitem", { name: "Clear all blocks…" })).toBeNull();
    expect(screen.getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
      "Split right",
      "Split down",
      "Clear output",
      "Copy all output",
      "Watch only",
    ]);
  });

  it("says the clear failed and leaves the blocks when the service refuses it", async () => {
    const user = await show([finished("b1", "npm test")]);
    mock.fail("/api/threads/th/terminals/t2/blocks", 1, 403, "desktop_required", "The desktop app is needed for this.");
    await openTabMenu(user);
    await clearFromMenu(user);
    await waitFor(() => expect(toasts()).toEqual(["Couldn't do that. The desktop app is needed for this."]));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByRole("group", { name: "Command npm test" })).toBeInTheDocument();
  });
});
