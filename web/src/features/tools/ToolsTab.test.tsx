import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { engineTools } from "../../../mock/fixtures";
import { api } from "../../api/client";
import { ApiFailure } from "../../api/client";
import { useWorkspace } from "../../product/state/workspace";
import { ToolsTab } from "./ToolsTab";

beforeEach(() => {
  useWorkspace.setState({
    engines: { data: [{ id: "claude", name: "Claude Code" }, { id: "codex", name: "Codex" }, { id: "opencode", name: "OpenCode" }] as never, error: null, loading: false },
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
  expect(await screen.findByText("No tools reported for this run")).toBeInTheDocument();
  expect(screen.getByText("Claude Code reports its tools when a run starts. They appear here after the next message.")).toBeInTheDocument();
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
  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText("Couldn’t read the tools.")).toBeInTheDocument();
  // The reason is the service's own words; nothing is red-and-grey at once.
  expect(within(alert).getByText("no")).toBeInTheDocument();
  expect(screen.queryByText(/Not reported by/)).toBeNull();
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

it("says why and keeps what was read last, dimmed with its time, when the check came back with an error", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue({ ...engineTools("codex"), error: "Codex didn’t answer in 10 seconds." });
  render(<ToolsTab threadId="t" engine="codex" />);
  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText("Couldn’t read the tools.")).toBeInTheDocument();
  expect(within(alert).getByText("Codex didn’t answer in 10 seconds.")).toBeInTheDocument();
  const last = screen.getByLabelText("Last read");
  expect(last).toHaveTextContent("Last read");
  expect(within(last).getByText("Orbyn tools")).toBeInTheDocument();
  expect(within(last).getByText("MCP servers")).toBeInTheDocument();
  // The list itself is not shown beside the error.
  expect(screen.queryByRole("region", { name: "Connectors" })).toBeNull();
});

it("says Not reported by the engine, in plain words with no error, for one that lists no tools", async () => {
  vi.spyOn(api, "threadTools").mockRejectedValue(new ApiFailure("unsupported", "This engine does not report its tools.", 422));
  vi.spyOn(api, "skills").mockResolvedValue({ skills: new Array(39).fill({}) } as never);
  render(<ToolsTab threadId="t" engine="opencode" />);
  expect(await screen.findByRole("heading", { name: "Not reported by OpenCode" })).toBeInTheDocument();
  expect(screen.getByText(/doesn’t list its tools to Orbyn, so there is nothing to show here\. Orbyn’s own skills are still available\./)).toBeInTheDocument();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(await screen.findByText("39")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Open Skills" })).toBeInTheDocument();
});

it("says a list was reported at run start only when nothing was asked of the engine, and keeps Check again otherwise", async () => {
  const read = vi.spyOn(api, "threadTools");
  read.mockResolvedValue({ ...engineTools("claude"), source: "run" });
  const first = render(<ToolsTab threadId="t" engine="claude" />);
  await screen.findByRole("region", { name: "Connectors" });
  expect(screen.getByText(/reported when this run started/)).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Check again" })).toBeNull();
  first.unmount();
  // Servers read from a live `claude mcp list`, the rest from a run's frame: it was checked, not just reported.
  read.mockResolvedValue({ ...engineTools("claude"), source: "check+run" });
  render(<ToolsTab threadId="t2" engine="claude" />);
  await screen.findByRole("region", { name: "Connectors" });
  expect(screen.queryByText(/reported when this run started/)).toBeNull();
  expect(screen.getByText(/Claude Code · checked/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Check again" })).toBeInTheDocument();
});

it("shows a list that was cut short, marked incomplete, not dimmed", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue({ ...engineTools("codex"), incomplete: true });
  const { container } = render(<ToolsTab threadId="t" engine="codex" />);
  expect(await screen.findByText("The list may be incomplete.")).toBeInTheDocument();
  expect(container.querySelector(".tg-stale")).toBeNull();
});
