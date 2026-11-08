import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { engineTools } from "../../../mock/fixtures";
import { api } from "../../api/client";
import { ApiFailure } from "../../api/client";
import { useWorkspace } from "../../state/workspace";
import { ToolsTab } from "./ToolsTab";

beforeEach(() => {
  useWorkspace.setState({
    engines: { data: [{ id: "claude", name: "Claude Code" }, { id: "codex", name: "Codex" }] as never, error: null, loading: false },
  });
});
afterEach(() => vi.restoreAllMocks());

it("lists the groups an engine reported, with each status in words and no switches", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue(engineTools("claude"));
  render(<ToolsTab threadId="t" engine="claude" />);
  const mcp = await screen.findByRole("region", { name: "Your MCP servers" });
  expect(within(mcp).getByText("github")).toBeInTheDocument();
  expect(within(mcp).getByText("Ready")).toBeInTheDocument();
  expect(within(mcp).getByText("Failed")).toBeInTheDocument();
  expect(within(mcp).getByText("Couldn’t start. Command not found.")).toBeInTheDocument();
  const connectors = screen.getByRole("region", { name: "Connectors" });
  expect(within(connectors).getByText("Needs sign-in")).toBeInTheDocument();
  expect(within(connectors).getByRole("link", { name: /Sign in/ })).toHaveAttribute("href", "https://claude.ai/settings/connectors");
  expect(screen.queryByRole("switch")).toBeNull();
  expect(screen.queryByRole("checkbox")).toBeNull();
  // Built-in tools and skills are listed by count; the groups start collapsed.
  expect(within(screen.getByRole("region", { name: "Skills" })).getByText("39")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Open Skills" })).toBeInTheDocument();
});

it("opens a failure's details on its row", async () => {
  const user = userEvent.setup();
  vi.spyOn(api, "threadTools").mockResolvedValue(engineTools("claude"));
  render(<ToolsTab threadId="t" engine="claude" />);
  await user.click(await screen.findByRole("button", { name: "Details" }));
  expect(screen.getByText("Command not found: pg-mcp")).toBeInTheDocument();
});

it("leaves out Codex's built-in group and says so in one quiet line", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue(engineTools("codex"));
  render(<ToolsTab threadId="t" engine="codex" />);
  await screen.findByRole("region", { name: "Connectors" });
  expect(screen.queryByRole("region", { name: "Built-in" })).toBeNull();
  expect(screen.getByText("Codex doesn’t report its built-in tools.")).toBeInTheDocument();
  expect(screen.getByText("Starting…")).toBeInTheDocument();
});

it("says there is nothing yet before the first run, and still offers Skills", async () => {
  const tools = { ...engineTools("claude"), reported: false, groups: [{ id: "skills", count: 39, items: [] }], not_reported: ["builtin", "orbyn", "mcp", "connectors", "plugins"] };
  vi.spyOn(api, "threadTools").mockResolvedValue(tools);
  render(<ToolsTab threadId="t" engine="claude" />);
  expect(await screen.findByText("No tools reported yet")).toBeInTheDocument();
  expect(screen.getByText(/Claude Code reports its tools when a run starts/)).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Open Skills" })).toBeInTheDocument();
});

it("shows a skeleton only if the answer takes longer than 400 ms, and an error with Try again", async () => {
  vi.useFakeTimers();
  try {
    let answer!: (value: never) => void;
    vi.spyOn(api, "threadTools").mockReturnValueOnce(new Promise((resolve) => (answer = resolve as never)));
    const { container } = render(<ToolsTab threadId="t" engine="claude" />);
    await act(() => vi.advanceTimersByTimeAsync(300));
    expect(container.querySelector(".tg-skel")).toBeNull();
    await act(() => vi.advanceTimersByTimeAsync(200));
    expect(container.querySelector(".tg-skel")).not.toBeNull();
    answer(engineTools("claude") as never);
    await act(() => vi.advanceTimersByTimeAsync(10));
    expect(container.querySelector(".tg-skel")).toBeNull();
  } finally {
    vi.useRealTimers();
  }
});

it("offers Try again when the engine does not answer", async () => {
  const user = userEvent.setup();
  const read = vi.spyOn(api, "threadTools").mockRejectedValueOnce(new ApiFailure("engine_failed", "no", 500));
  render(<ToolsTab threadId="t" engine="claude" />);
  expect(await screen.findByText("Couldn’t read the tools")).toBeInTheDocument();
  read.mockResolvedValue(engineTools("claude"));
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(screen.getByRole("region", { name: "Connectors" })).toBeInTheDocument());
});

it("Check again asks the engine itself, not a kept answer", async () => {
  const user = userEvent.setup();
  const read = vi.spyOn(api, "threadTools").mockResolvedValue(engineTools("codex"));
  render(<ToolsTab threadId="t" engine="codex" />);
  await screen.findByRole("region", { name: "Connectors" });
  expect(read).toHaveBeenLastCalledWith("t", false, expect.anything());
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(read).toHaveBeenLastCalledWith("t", true));
});

it("keeps the list and says why when the engine's check came back with an error", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue({ ...engineTools("codex"), error: "Codex's list may be incomplete." });
  render(<ToolsTab threadId="t" engine="codex" />);
  expect(await screen.findByText(/Codex's list may be incomplete/)).toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Connectors" })).toBeInTheDocument();
});
