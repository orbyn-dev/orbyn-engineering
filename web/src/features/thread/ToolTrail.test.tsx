import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TimelineRow } from "./items";
import { TimelineThread } from "./timelineThread";
import { rangesIn } from "./ThreadFind";
import type { ToolItem } from "./projection";
import { entryText } from "./threadSearch";
import { callFindText } from "./toolCalls";
import { groupWork } from "./timeline";
import { forgetOpenState } from "../../ui/openState";

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
      {groupWork(items).map((entry) => (
        <TimelineRow key={entry.key} entry={entry} live={live} tools={[]} />
      ))}
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
  it("shows a finished command group with its duration and opens to the command row", async () => {
    render(<Trail items={[make("Bash", { command: "git status --short" }, { exitCode: 0 })]} />);
    expect(screen.getByRole("button", { name: "Ran 1 command 3.9s" })).toHaveAttribute("aria-expanded", "false");
    await userEvent.click(screen.getByRole("button", { name: "Ran 1 command 3.9s" }));
    expect(screen.getByRole("button", { name: "git status --short 3.9s" })).toBeInTheDocument();
  });

  it("marks a failed command in the row and says why on the collapsed group", () => {
    render(<Trail items={[make("Bash", { command: "npm test" }, { exitCode: 1, result: "1 failed" })]} />);
    const group = screen.getByRole("button", { name: "Ran 1 command npm test exit 1 · 3.9s" });
    expect(group).toHaveClass("s-fail");
    expect(group).toHaveTextContent("exit 1 · 3.9s");
  });

  it("shows a running command with a spinner and the current command in the collapsed group", () => {
    const running = make("Bash", { command: "npm test" }, { result: null, done: false, endedAt: undefined, output: "line" });
    render(<Trail items={[running]} live />);
    const group = rows()[0]!;
    expect(group).toHaveClass("s-run");
    expect(group).toHaveTextContent("Running");
    expect(group).toHaveTextContent("npm test");
    expect(group.querySelector(".spin")).not.toBeNull();
  });

  it("caps long output at twelve lines and shows the rest on request", async () => {
    const output = Array.from({ length: 20 }, (_, n) => `line ${n}`).join("\n");
    render(<Trail items={[make("Bash", { command: "seq 20" }, { exitCode: 0, result: output })]} />);
    await openAll();
    expect(document.querySelector(".co.cap")).not.toBeNull();
    await userEvent.click(screen.getByRole("button", { name: /Show all 20 lines/ }));
    expect(document.querySelector(".co.cap")).toBeNull();
    expect(screen.getByRole("button", { name: /Show less/ })).toBeInTheDocument();
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

  it("moves between rows with the arrow keys, opens with Right and shows the key hint", async () => {
    render(
      <Trail
        items={[make("Bash", { command: "a" }, { exitCode: 0 }), make("Bash", { command: "b" }, { exitCode: 0 }), make("Read", { file_path: "/x/f.ts" })]}
      />,
    );
    const user = userEvent.setup();
    await user.tab();
    expect(rows()[0]).toHaveFocus();
    expect(screen.getByText("Copy command")).toBeInTheDocument();
    await user.keyboard("{ArrowRight}");
    expect(rows()[0]).toHaveAttribute("aria-expanded", "true");
    await user.keyboard("{ArrowDown}");
    expect(rows()[1]).toHaveFocus();
    await user.keyboard("{ArrowLeft}");
    expect(rows()[0]).toHaveAttribute("aria-expanded", "true");
  });

  it("opens the call's stored record from the footer", async () => {
    render(<Trail items={[make("Bash", { command: "ls" }, { exitCode: 0 })]} />);
    await openAll();
    await userEvent.click(screen.getByRole("button", { name: "Inspect item" }));
    expect(await screen.findByRole("dialog", { name: "Item details" })).toBeInTheDocument();
  });

  it("draws an edit as a small diff with its counts on the row", async () => {
    render(<Trail items={[make("Edit", { file_path: "/p/auth/redirect.ts", old_string: 'return "/home"', new_string: "const next = 1\nreturn next" })]} />);
    expect(rows()[0]).toHaveTextContent("+2 −1");
    await userEvent.click(rows()[0]!);
    expect(document.querySelectorAll(".hunk .add")).toHaveLength(2);
    expect(document.querySelectorAll(".hunk .del")).toHaveLength(1);
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

  it("names an MCP call by server then tool and lists its arguments as pairs", async () => {
    render(<Trail items={[make("mcp__orbyn-memory__find", { query: "login redirect", limit: 5 }, { result: "2 entries" })]} />);
    expect(rows()[0]).toHaveTextContent("orbyn-memory");
    expect(rows()[0]).toHaveTextContent("find");
    await userEvent.click(rows()[0]!);
    expect(document.querySelector(".kvs")).toHaveTextContent("login redirect");
    expect(screen.getByRole("button", { name: "Raw result" })).toBeInTheDocument();
  });
});

describe("find counts what a block shows", () => {
  const cases: [string, ToolItem][] = [
    ["a command", make("Bash", { command: "echo needle" }, { exitCode: 0, result: "needle out\nplain" })],
    ["a read", make("Read", { file_path: "/x/f.ts" }, { result: "     1→const needle = 1\n     2→const b = 2" })],
    ["an edit", make("Edit", { file_path: "/x/f.ts", old_string: "old needle", new_string: "new needle" })],
    ["a new file", make("Write", { file_path: "/x/f.ts", content: "needle\nsecond" }, { result: "File created successfully at: /x/f.ts" })],
    ["a search", make("Grep", { pattern: "needle" }, { result: "src/a.ts:1:needle here\nsrc/b.ts:2:other" })],
    ["a listing", make("LS", { path: "/x" }, { result: "- /x/\n  - needle.ts\n  - other.ts" })],
    ["a page fetch", make("WebFetch", { url: "https://example.com/needle" }, { result: "A needle excerpt" })],
    ["an MCP call", make("mcp__srv__tool", { query: "needle" }, { result: "needle result" })],
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
