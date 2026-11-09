import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TimelineRow } from "./items";
import { TimelineThread } from "./timelineThread";
import { rangesIn } from "./ThreadFind";
import type { ToolItem } from "../../lib/timeline/types";
import { entryText } from "./threadSearch";
import { callFindText } from "./toolCalls";
import { forgetOpenState, OpenStateScope } from "../../ui/openState";
import { arrangeTimeline, groupWork } from "../../lib/timeline/timeline";
import { useUi } from "../../state/ui";

const make = (name: string, args: unknown, extra: Partial<ToolItem> = {}): ToolItem => ({
  kind: "tool",
  key: `tool:${name}:${Math.random()}`,
  callId: "c",
  seq: 1,
  runId: "r",
  ts: "2026-11-08T10:00:00.000Z",
  endedAt: "2026-11-08T10:00:03.900Z",
  name,
  args: JSON.stringify(args),
  output: "",
  result: "ok",
  done: true,
  exitCode: null,
  ...extra,
});

function Trail({ items, live = false }: { items: ToolItem[]; live?: boolean }) {
  return (
    <TimelineThread value="t1">
      <OpenStateScope value="t1">{groupWork(items).map((entry) => (
        <TimelineRow key={entry.key} entry={entry} live={live} tools={[]} />
      ))}</OpenStateScope>
    </TimelineThread>
  );
}

const openAll = async () => {
  for (let pass = 0; pass < 3; pass++) for (const row of rows()) if (row.getAttribute("aria-expanded") === "false") await act(async () => row.click());
};
const rows = () => [...document.querySelectorAll<HTMLButtonElement>("button.tcr")];

beforeEach(() => forgetOpenState());
afterEach(() => vi.restoreAllMocks());

describe("the trail of tool calls", () => {
  it("shows one command as one row with its duration, and one click shows its output", async () => {
    render(<Trail items={[make("Bash", { command: "git status --short" }, { exitCode: 0, result: " M a.ts" })]} />);
    const row = screen.getByRole("button", { name: "git status --short 3.9s" });
    expect(rows()).toHaveLength(1);
    await userEvent.click(row);
    expect(screen.getByText("M a.ts")).toBeInTheDocument();
  });

  it("marks a failed command with its exit code, and says so to a screen reader", () => {
    render(<Trail items={[make("Bash", { command: "npm test" }, { exitCode: 1, result: "1 failed" })]} />);
    const row = screen.getByRole("button", { name: "npm test exit 1 · 3.9s, failed" });
    expect(row).toHaveClass("s-fail");
    expect(row).toHaveTextContent("exit 1 · 3.9s");
  });

  it("shows a running command with a spinner, and says it is running", () => {
    const running = make("Bash", { command: "npm test" }, { result: null, done: false, endedAt: undefined, output: "line" });
    render(<Trail items={[running]} live />);
    expect(rows()[0]).toHaveClass("s-run");
    expect(rows()[0]?.querySelector(".spin")).not.toBeNull();
    expect(rows()[0]).toHaveAccessibleName(/, running$/);
  });

  it("shows a call that never finished in a run that is over as stopped", () => {
    render(<Trail items={[make("Bash", { command: "sleep 9" }, { result: null, done: false, endedAt: undefined })]} />);
    expect(rows()[0]).toHaveClass("s-stop");
    expect(rows()[0]).toHaveAccessibleName(/, stopped$/);
  });

  it("groups two commands, and the collapsed group names the one that failed", () => {
    render(
      <Trail
        items={[make("Bash", { command: "ls" }, { exitCode: 0 }), make("Bash", { command: "npm test" }, { exitCode: 1, result: "1 failed" })]}
      />,
    );
    const group = screen.getByRole("button", { name: "Ran 2 commands npm test exit 1 · 7.8s, failed" });
    expect(group).toHaveClass("s-fail");
  });

  it("starts a capped output at its first line, not its end", async () => {
    const output = Array.from({ length: 20 }, (_, n) => `line ${n}`).join("\n");
    render(<Trail items={[make("Bash", { command: "seq 20" }, { exitCode: 0, result: output })]} />);
    await openAll();
    expect(document.querySelector(".co.cap")).not.toBeNull();
    expect((document.querySelector(".co.cap") as HTMLElement).scrollTop).toBe(0);
    await userEvent.click(screen.getByRole("button", { name: /Show all 20 lines/ }));
    expect(document.querySelector(".co.cap")).toBeNull();
    expect(screen.getByRole("button", { name: /Show less/ })).toBeInTheDocument();
  });

  it("resets a capped block to its first line when a watched run ends, and after Show less", async () => {
    const output = Array.from({ length: 20 }, (_, n) => `line ${n}`).join("\n");
    const live = make("Bash", { command: "seq 20" }, { result: null, done: false, endedAt: undefined, output });
    const view = render(<Trail items={[live]} live />);
    await openAll();
    const block = () => document.querySelector(".co") as HTMLElement;
    block().scrollTop = 400;
    view.rerender(<Trail items={[{ ...live, result: output, done: true, exitCode: 0, endedAt: "2026-11-08T10:00:04.000Z" }]} />);
    expect(block()).toHaveClass("cap");
    expect(block().scrollTop).toBe(0);
    await userEvent.click(screen.getByRole("button", { name: /Show all 20 lines/ }));
    block().scrollTop = 120;
    await userEvent.click(screen.getByRole("button", { name: /Show less/ }));
    expect(block()).toHaveClass("cap");
    expect(block().scrollTop).toBe(0);
  });

  it("offers Wrap lines only when a line is wider than the block", async () => {
    const width = vi.spyOn(HTMLElement.prototype, "scrollWidth", "get");
    width.mockReturnValue(0);
    const first = render(<Trail items={[make("Bash", { command: "echo" }, { exitCode: 0, result: "short" })]} />);
    await openAll();
    expect(screen.queryByRole("button", { name: /Wrap lines/ })).toBeNull();
    first.unmount();
    forgetOpenState();
    width.mockImplementation(function (this: HTMLElement) {
      return this.classList.contains("co") ? 900 : 0;
    });
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockReturnValue(300);
    render(<Trail items={[make("Bash", { command: "echo" }, { exitCode: 0, result: "x".repeat(400) })]} />);
    await openAll();
    expect(await screen.findByRole("button", { name: /Wrap lines/ })).toBeInTheDocument();
  });

  it("says when a command printed nothing, and keeps the footer", async () => {
    render(<Trail items={[make("Bash", { command: "true" }, { exitCode: 0, result: "" })]} />);
    await openAll();
    expect(screen.getByText("No output")).toBeInTheDocument();
    expect(document.querySelector(".cf")).toHaveTextContent("exit 0");
  });

  it("copies the command and says Copied for a moment", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<Trail items={[make("Bash", { command: "git status" }, { exitCode: 0 })]} />);
    await openAll();
    await userEvent.click(screen.getByRole("button", { name: "Copy command" }));
    expect(writeText).toHaveBeenCalledWith("git status");
    expect(await screen.findByText("Copied")).toBeInTheDocument();
  });

  it("moves between rows with the arrow keys, opens with Right, closes with Left, and shows a key hint", async () => {
    render(<Trail items={[make("Bash", { command: "a" }, { exitCode: 0 }), make("Read", { file_path: "/x/f.ts" }), make("Grep", { pattern: "q" })]} />);
    const user = userEvent.setup();
    await user.tab();
    expect(rows()[0]).toHaveFocus();
    expect(screen.getByText("Copy command")).toBeInTheDocument();
    await user.keyboard("{ArrowRight}");
    expect(rows()[0]).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{ArrowLeft}");
    expect(rows()[0]).toHaveAttribute("aria-expanded", "false");
    await user.keyboard("{ArrowDown}");
    expect(rows()[1]).toHaveFocus();
    expect(screen.queryByText("Copy command")).toBeNull();
  });

  it("copies the focused command with C and says Copied", async () => {
    const user = userEvent.setup();
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    render(<Trail items={[make("Bash", { command: "git status" }, { exitCode: 0 })]} />);
    await user.tab();
    await user.keyboard("c");
    expect(writeText).toHaveBeenCalledWith("git status");
    expect(await screen.findByRole("status")).toHaveTextContent("Copied");
  });

  it("draws an edit as a small diff with its counts on the row", async () => {
    render(<Trail items={[make("Edit", { file_path: "/p/auth/redirect.ts", old_string: 'return "/home"', new_string: "const next = 1\nreturn next" })]} />);
    expect(rows()[0]).toHaveTextContent("+2 −1");
    await userEvent.click(rows()[0]!);
    expect(document.querySelectorAll(".hunk .add")).toHaveLength(2);
    expect(document.querySelectorAll(".hunk .del")).toHaveLength(1);
  });

  it("counts the files an edit group touched, not its calls", () => {
    const edit = (n: number) => make("Edit", { file_path: "/p/same.ts", old_string: `a${n}`, new_string: `b${n}` });
    render(<Trail items={[edit(1), edit(2), edit(3)]} />);
    expect(rows()[0]).toHaveTextContent("Edited 1 file");
  });

  it("reads a block of a read: numbered lines and Open file", async () => {
    render(<Trail items={[make("Read", { file_path: "/p/a.ts", offset: 12, limit: 2 }, { result: "    12→const a = 1\n    13→const b = 2" })]} />);
    expect(rows()[0]).toHaveTextContent("· lines 12–13");
    await userEvent.click(rows()[0]!);
    expect(screen.getByText("const a = 1")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Open file" })).toBeInTheDocument();
  });

  it("shows a new file's first lines with its total", async () => {
    render(<Trail items={[make("Write", { file_path: "/p/n.ts", content: "one\ntwo\nthree" }, { result: "File created successfully at: /p/n.ts" })]} />);
    expect(rows()[0]).toHaveTextContent("Created");
    await userEvent.click(rows()[0]!);
    expect(screen.getByText("two")).toBeInTheDocument();
    expect(document.querySelector(".cf")).toHaveTextContent("New file · 3 lines");
  });

  it("lists search results as path and text, capped at seven with the rest on request", async () => {
    const result = Array.from({ length: 10 }, (_, n) => `src/f${n}.ts:${n + 1}:match ${n}`).join("\n");
    render(<Trail items={[make("Grep", { pattern: "match" }, { result })]} />);
    expect(rows()[0]).toHaveTextContent("Searched for “match”");
    expect(rows()[0]).toHaveTextContent("· 10 results");
    await userEvent.click(rows()[0]!);
    expect(document.querySelector(".rs.cap7")).not.toBeNull();
    await userEvent.click(screen.getByRole("button", { name: /Show all 10 results/ }));
    expect(document.querySelector(".rs.cap7")).toBeNull();
  });

  it("lists a folder's entries, capped at six", async () => {
    const result = ["- /p/", ...Array.from({ length: 9 }, (_, n) => `  - file${n}.ts`)].join("\n");
    render(<Trail items={[make("LS", { path: "/p" }, { result })]} />);
    await userEvent.click(rows()[0]!);
    expect(document.querySelector(".rs.cap6")).not.toBeNull();
    expect(screen.getByRole("button", { name: /Show all \d+ entries/ })).toBeInTheDocument();
  });

  it("names an MCP call by server then tool and lists its arguments as pairs", async () => {
    render(<Trail items={[make("mcp__orbyn-memory__find", { query: "login redirect", limit: 5 }, { result: "2 entries" })]} />);
    expect(rows()[0]).toHaveTextContent("orbyn-memory");
    await userEvent.click(rows()[0]!);
    expect(document.querySelector(".kvs")).toHaveTextContent("login redirect");
  });

  it("shows a subagent's task, falling back to its prompt, and its result", async () => {
    render(<Trail items={[make("Task", { subagent_type: "Explore", description: "", prompt: "Find every reader" }, { result: "4 files" })]} />);
    await userEvent.click(rows()[0]!);
    expect(document.querySelector(".kvs")).toHaveTextContent("Find every reader");
    expect(document.querySelector(".kvs")).toHaveTextContent("4 files");
  });

  it("opens a fetched page in the Browser only for an http or https address", async () => {
    const showInBrowser = vi.fn();
    const state = useUi.getState();
    vi.spyOn(useUi, "getState").mockReturnValue({ ...state, showInBrowser });
    vi.spyOn(useUi, "getState");
    for (const [url, shown] of [["https://example.com/a", true], ["file:///etc/passwd", false], ["javascript:alert(1)", false], ["chrome://settings", false]] as const) {
      forgetOpenState();
      const view = render(<Trail items={[make("WebFetch", { url }, { result: "text" })]} />);
      await userEvent.click(rows()[0]!);
      expect(screen.queryByRole("button", { name: "Open in Browser" }) !== null, url).toBe(shown);
      view.unmount();
    }
  });

  it("groups several searches, MCP calls or fetches into one counted row", () => {
    render(
      <Trail
        items={[make("Grep", { pattern: "a" }), make("Grep", { pattern: "b" }), make("Grep", { pattern: "c" }), make("mcp__s__t", {}), make("mcp__s__t", {}), make("WebFetch", { url: "https://a.b" }), make("WebFetch", { url: "https://c.d" })]}
      />,
    );
    const names = rows().map((row) => row.textContent);
    expect(names.some((name) => name?.includes("Searched the code 3 times"))).toBe(true);
    expect(names.some((name) => name?.includes("2 MCP calls"))).toBe(true);
    expect(names.some((name) => name?.includes("Fetched 2 pages"))).toBe(true);
  });

  it("opens the group with it when a second call joins a row that was open", async () => {
    const first = make("Read", { file_path: "/p/a.ts" }, { result: "    1→a" });
    const view = render(<Trail items={[first]} />);
    await userEvent.click(rows()[0]!);
    expect(rows()[0]).toHaveAttribute("aria-expanded", "true");
    view.rerender(<Trail items={[first, make("Read", { file_path: "/p/b.ts" }, { result: "    1→b" })]} />);
    expect(rows()[0]).toHaveTextContent("Read 2 files");
    expect(rows()[0]).toHaveAttribute("aria-expanded", "true");
    expect(rows()[1]).toHaveAttribute("aria-expanded", "true");
  });

  it("opens a new group for an open row after the rows remounted, and leaves a group someone closed shut", async () => {
    const first = make("Read", { file_path: "/p/a.ts" }, { result: "    1→a" });
    const second = make("Read", { file_path: "/p/b.ts" }, { result: "    1→b" });
    const alone = render(<Trail items={[first]} />);
    await userEvent.click(rows()[0]!);
    alone.unmount();
    // The thread's open state is kept; the rows mount again, now as a group.
    const joined = render(<Trail items={[first, second]} />);
    expect(rows()[0]).toHaveAttribute("aria-expanded", "true");
    await userEvent.click(rows()[0]!);
    expect(rows()[0]).toHaveAttribute("aria-expanded", "false");
    joined.unmount();
    render(<Trail items={[first, second]} />);
    expect(rows()[0]).toHaveAttribute("aria-expanded", "false");
  });

  it("opens the call's raw result from the footer, on every kind that has one", async () => {
    render(<Trail items={[make("Bash", { command: "ls" }, { exitCode: 0 })]} />);
    await openAll();
    await userEvent.click(screen.getByRole("button", { name: "Raw result" }));
    expect(await screen.findByRole("dialog", { name: "Item details" })).toBeInTheDocument();
  });
});

describe("Expand all and Collapse all", () => {
  const kinds = [
    make("Bash", { command: "a" }, { exitCode: 0 }),
    make("Read", { file_path: "/p/a.ts" }),
    make("Grep", { pattern: "q" }),
    make("Edit", { file_path: "/p/a.ts", old_string: "a", new_string: "b" }),
  ];
  const turn = (items: ToolItem[]) => [
    { kind: "message" as const, key: "u", seq: 0, runId: "r", ts: "", role: "user", text: "go", done: true },
    ...items,
    { kind: "message" as const, key: "a", seq: 99, runId: "r", ts: "", role: "assistant", text: "done", done: true },
  ];
  const renderTurn = (items: ToolItem[]) =>
    render(
      <TimelineThread value="t1">
        <OpenStateScope value="t1">
          {arrangeTimeline(turn(items), {}, null).map((entry) => (
            <TimelineRow key={entry.key} entry={entry} live={false} tools={[]} />
          ))}
        </OpenStateScope>
      </TimelineThread>,
    );

  it("opens and folds every group of a turn with four or more", async () => {
    renderTurn(kinds);
    await userEvent.click(document.querySelector<HTMLElement>("button.work-summary")!);
    expect(screen.getByText("4 tool calls in 4 groups")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Expand all" }));
    expect(rows().map((row) => row.getAttribute("aria-expanded"))).toEqual(["true", "true", "true", "true"]);
    await userEvent.click(screen.getByRole("button", { name: "Collapse all" }));
    expect(rows().map((row) => row.getAttribute("aria-expanded"))).toEqual(["false", "false", "false", "false"]);
  });

  it("targets the rows the view shows when reasoning splits a run, and counts them as groups", async () => {
    const reasoning = { kind: "reasoning" as const, key: "think", seq: 3, runId: "r", ts: "", text: "hm", done: true };
    const a = make("Read", { file_path: "/p/a.ts" });
    const b = make("Read", { file_path: "/p/b.ts" });
    renderTurn([a, reasoning, b, make("Bash", { command: "x" }, { exitCode: 0 }), make("Grep", { pattern: "q" })] as never);
    await userEvent.click(document.querySelector<HTMLElement>("button.work-summary")!);
    expect(screen.getByText("4 tool calls in 4 groups")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Expand all" }));
    const open = rows().filter((row) => row.getAttribute("aria-expanded") === "true");
    expect(open.some((row) => row.textContent?.includes("a.ts"))).toBe(true);
    expect(open.some((row) => row.textContent?.includes("b.ts"))).toBe(true);
    expect(open.some((row) => row.textContent?.includes("Read 2 files"))).toBe(false);
  });

  it("is not offered for a turn with fewer than four groups", async () => {
    renderTurn(kinds.slice(0, 3));
    await userEvent.click(document.querySelector<HTMLElement>("button.work-summary")!);
    expect(screen.queryByRole("button", { name: "Expand all" })).toBeNull();
  });
});

describe("find counts what a block shows", () => {
  const cases: [string, ToolItem][] = [
    ["a command", make("Bash", { command: "echo needle" }, { exitCode: 0, result: "needle out\nplain" })],
    ["a command with no text", make("Bash", { command: "" }, { exitCode: 0, result: "needle" })],
    ["a read", make("Read", { file_path: "/x/f.ts" }, { result: "     1→const needle = 1\n     2→const b = 2" })],
    ["an edit", make("Edit", { file_path: "/x/f.ts", old_string: "old needle", new_string: "new needle" })],
    ["a new file", make("Write", { file_path: "/x/f.ts", content: "needle\nsecond" }, { result: "File created successfully at: /x/f.ts" })],
    ["a search", make("Grep", { pattern: "needle" }, { result: "src/a.ts:1:needle here\nsrc/b.ts:2:other" })],
    ["a listing", make("LS", { path: "/x" }, { result: "- /x/\n  - needle.ts\n  - other.ts" })],
    ["a page fetch", make("WebFetch", { url: "https://example.com/needle" }, { result: "A \u001b[32mneedle\u001b[0m excerpt" })],
    ["an MCP call", make("mcp__srv__tool", { query: "needle" }, { result: "\u001b[1mneedle\u001b[0m result" })],
    ["a subagent", make("Task", { description: "", prompt: "needle prompt" }, { result: "\u001b[31mneedle\u001b[0m done" })],
    ["another tool", make("Mystery", { subject: "needle" }, { result: "needle result" })],
  ];
  it.each(cases)("%s: the highlights match the count", async (_name, item) => {
    const entries = groupWork([item]);
    render(<Trail items={[item]} />);
    await openAll();
    const counted = entries.map(entryText).join("\n").split("needle").length - 1;
    const painted = rangesIn(document.body, "needle").length;
    expect(painted).toBe(counted);
    expect(callFindText(item).join("\n").toLowerCase().split("needle").length - 1).toBe(counted);
  });
});
