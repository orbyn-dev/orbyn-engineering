import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useConnection } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import { levelKey, useExplorer } from "../../../workspace";
import { clearReportedCommands, publishFinished } from "../shellOutcomes";
import { registerTerminal, type TerminalControls } from "../terminalControls";
import { TerminalInput } from "./TerminalInput";

const ROOT = "/w/tree";
let written: string[] = [];
let stop: () => void = () => undefined;
/** Each case gets its own terminal, so one case's saved lines cannot show in another's list. */
const keys = ["th:one", "th:two", "th:three", "th:four"];
let keyAt = 0;

const controls = (): TerminalControls => ({
  clear: () => undefined,
  write: (data) => written.push(data),
  interrupt: () => undefined,
  output: () => "",
  find: () => undefined,
  onResults: () => () => undefined,
});

/** Runs a line in the shell, as the person does, and says how the shell then reported it ended. */
const setup = async (command: string) => {
  const key = keys[keyAt] as string;
  keyAt += 1;
  written = [];
  stop = registerTerminal(key, controls());
  useWorkspace.setState({
    threads: { th: { id: "th", project_id: "p", engine: "claude", model: "Sonnet", active_run: null } },
    engines: { data: [{ id: "claude", name: "Claude", installed: true, auth: "logged_in" }] },
  } as never);
  useExplorer.setState({
    worktrees: { p: { data: { project_id: "p", git: true, worktrees: [{ id: "w1", path: ROOT, threads: [{ id: "th" }] }] }, loading: false, error: null } },
    levels: { [levelKey("p", "w1", "")]: { data: { entries: [{ name: "src", kind: "directory" }] }, loading: false, error: null } },
    loadWorktrees: vi.fn(() => Promise.resolve()),
    loadLevel: vi.fn(() => Promise.resolve()),
  } as never);
  render(<TerminalInput threadId="th" keyOf={key} />);
  const field = screen.getByLabelText("Terminal input") as HTMLTextAreaElement;
  const user = userEvent.setup();
  await user.type(field, command);
  await user.keyboard("{Enter}");
  expect(written).toEqual([`${command}\r`]);
  return { key, user };
};

const openHistory = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.click(screen.getByRole("button", { name: /History/ }));
  return screen.findByRole("option");
};

describe("a history row shows how the shell said the line ended", () => {
  beforeEach(() => useConnection.setState({ offline: false, serviceDown: false }));
  afterEach(() => {
    stop();
    for (const key of keys) clearReportedCommands(key);
    window.localStorage.clear();
  });

  it("shows a failure with its mark and the failed count", async () => {
    const { key, user } = await setup("npm test");
    act(() => publishFinished(key, [{ seq: 0, command: "npm test", exitCode: 1 }]));
    const row = await openHistory(user);
    expect(row.querySelector(".term-input-when")?.textContent).toMatch(/ · 1 failed$/);
    expect(row.querySelector(".term-input-bad")).not.toBeNull();
    expect(row.querySelector(".term-input-ok")).toBeNull();
  });

  it("shows a success with its mark and no failure words", async () => {
    const { key, user } = await setup("npm run build");
    act(() => publishFinished(key, [{ seq: 0, command: "npm run build", exitCode: 0 }]));
    const row = await openHistory(user);
    expect(row.querySelector(".term-input-ok")).not.toBeNull();
    expect(row.querySelector(".term-input-bad")).toBeNull();
    expect(row.querySelector(".term-input-when")?.textContent).not.toContain("failed");
  });

  it("leaves the row without a mark when the shell reports no code for it", async () => {
    const { key, user } = await setup("ls");
    act(() => publishFinished(key, [{ seq: 0, command: "ls", exitCode: null }]));
    const row = await openHistory(user);
    expect(row.querySelector(".term-input-ok, .term-input-bad")).toBeNull();
    expect(row.querySelector(".term-input-when")?.textContent).not.toContain("failed");
  });

  it("does not mark a row for a command the editor never ran", async () => {
    const { key, user } = await setup("ls");
    act(() => publishFinished(key, [{ seq: 0, command: "cargo test", exitCode: 1 }]));
    const row = await openHistory(user);
    expect(row.querySelector(".term-input-ok, .term-input-bad")).toBeNull();
  });
});
