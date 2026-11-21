import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useConnection } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import { levelKey, useExplorer } from "../../../workspace";
import { clearShellFolder, setShellFolder } from "../shellFolder";
import { recordCommand } from "./history";
import { TerminalInput } from "./TerminalInput";

const ROOT = "/w/tree";
const level = (...names: [string, "file" | "directory"][]) => ({ data: { entries: names.map(([name, kind]) => ({ name, kind })) }, loading: false, error: null });
const loadLevel = vi.fn(() => Promise.resolve());

const setup = (key: string) => {
  useWorkspace.setState({
    threads: { th: { id: "th", project_id: "p", engine: "claude", model: "Sonnet", active_run: null } },
    engines: { data: [{ id: "claude", name: "Claude", installed: true, auth: "logged_in" }] },
  } as never);
  useExplorer.setState({
    worktrees: { p: { data: { project_id: "p", git: true, worktrees: [{ id: "w1", path: ROOT, threads: [{ id: "th" }] }] }, loading: false, error: null } },
    levels: {
      [levelKey("p", "w1", "")]: level(["README.md", "file"], ["src", "directory"]),
      [levelKey("p", "w1", "src")]: level(["main.tsx", "file"], ["lib", "directory"]),
      [levelKey("p", "w1", "src/lib")]: level(["util.ts", "file"]),
    },
    loadWorktrees: vi.fn(() => Promise.resolve()),
    loadLevel,
  } as never);
  render(<TerminalInput threadId="th" keyOf={key} />);
  return { field: screen.getByLabelText("Terminal input") as HTMLTextAreaElement, user: userEvent.setup() };
};
const shown = () => screen.queryAllByRole("option").map((item) => item.querySelector("code")?.textContent);

describe("path completion follows the shell's folder", () => {
  beforeEach(() => {
    loadLevel.mockClear();
    useConnection.setState({ offline: false, serviceDown: false });
  });
  afterEach(() => {
    for (const key of ["th:main", "th:two"]) clearShellFolder(key);
  });

  it("starts from the worktree root until the shell reports a folder", async () => {
    const { field, user } = setup("th:main");
    await user.type(field, "cat ");
    await user.keyboard("{Tab}");
    expect(shown()).toEqual(expect.arrayContaining(["README.md", "src/"]));
    expect(loadLevel).not.toHaveBeenCalled();
  });

  it("completes relative paths against the reported folder, and follows it when it changes", async () => {
    const { field, user } = setup("th:main");
    act(() => setShellFolder("th:main", `${ROOT}/src`));
    await user.type(field, "cat m");
    expect(shown()).toEqual(["main.tsx"]);
    act(() => setShellFolder("th:main", `${ROOT}/src/lib`));
    await user.clear(field);
    await user.type(field, "cat u");
    expect(shown()).toEqual(["util.ts"]);
  });

  it("reads a typed folder relative to the reported one", async () => {
    const { field, user } = setup("th:main");
    act(() => setShellFolder("th:main", `${ROOT}/src`));
    await user.type(field, "cat lib/");
    expect(shown()).toEqual(["util.ts"]);
  });

  it("ignores a folder outside the worktree: nothing outside is listed, the start folder is used", async () => {
    const { field, user } = setup("th:main");
    act(() => setShellFolder("th:main", "/etc"));
    await user.type(field, "cat s");
    expect(shown()).toEqual(["src/"]);
    for (const call of loadLevel.mock.calls as unknown as unknown[][]) expect(String(call[2])).not.toContain("etc");
  });

  it("keeps each terminal's folder to itself", async () => {
    act(() => setShellFolder("th:two", `${ROOT}/src`));
    const { field, user } = setup("th:main");
    await user.type(field, "cat s");
    expect(shown()).toEqual(["src/"]);
  });

  it("does not touch history or recall: a multi-line command comes back whole", async () => {
    recordCommand("th:main", "echo one\necho two");
    const { user, field } = setup("th:main");
    await user.click(screen.getByRole("button", { name: /History/ }));
    expect(await screen.findByText("echo one ⏎ echo two")).toBeTruthy();
    await user.keyboard("{Enter}");
    expect(field.value).toBe("echo one\necho two");
  });
});
