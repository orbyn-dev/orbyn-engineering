import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import { engineTools } from "../../../mock/fixtures";
import { App } from "../../app/App";
import { installMockFetch } from "../../test-support";
import { useUi } from "../../state/ui";

beforeEach(() => {
  installMockFetch();
  useUi.setState({ projectFolders: true, panelOpen: true, panelTab: "diff" });
  useUi.getState().setPrefs({ welcomeDone: true });
  window.location.hash = "#/threads/t-done";
});

it("the details card's Tools row counts what is ready, names what is not, and opens the Tools tab", async () => {
  const user = userEvent.setup();
  render(<App />);
  const row = await screen.findByRole("button", { name: "Tools" });
  await waitFor(() => expect(row).toHaveTextContent("8 ready"));
  expect(row).toHaveTextContent("1 needs sign-in · 1 failed");
  // The tab is not in the row until it is asked for.
  expect(screen.queryByRole("tab", { name: "Tools" })).toBeNull();
  await user.click(row);
  const tab = await screen.findByRole("tab", { name: "Tools" });
  expect(tab).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByRole("region", { name: "Your MCP servers" })).toBeInTheDocument();
  // It closes like any other tab.
  await user.click(screen.getByRole("button", { name: "Close Tools tab" }));
  expect(screen.queryByRole("tab", { name: "Tools" })).toBeNull();
});

it("says Not reported yet on the card before an engine has reported anything", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue({ ...engineTools("claude"), reported: false, groups: [], not_reported: [] });
  render(<App />);
  const row = await screen.findByRole("button", { name: "Tools" });
  await waitFor(() => expect(row).toHaveTextContent("Not reported yet"));
});

it("Settings › Agents lists Tools and connectors, one tab per engine, read-only, with Check again", async () => {
  const user = userEvent.setup();
  const read = vi.spyOn(api, "engineTools").mockImplementation(async (engine) => engineTools(engine));
  window.location.hash = "#/settings/tools";
  render(<App />);
  const claude = await screen.findByRole("tab", { name: "Claude Code" });
  expect(claude).toHaveAttribute("aria-selected", "true");
  expect(await screen.findByText(/Read from Claude Code when each run starts/)).toBeInTheDocument();
  expect((await screen.findAllByText("claude.ai connector", { exact: false })).length).toBeGreaterThan(0);
  expect(screen.queryByRole("switch")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(read).toHaveBeenCalledWith("claude", true));
  await user.click(screen.getByRole("tab", { name: "Codex" }));
  expect(await screen.findByText("Codex doesn’t report its built-in tools.")).toBeInTheDocument();
});

it("keeps the last list, dimmed, when an engine does not answer the check", async () => {
  const user = userEvent.setup();
  const read = vi.spyOn(api, "engineTools").mockImplementation(async (engine) => engineTools(engine));
  window.location.hash = "#/settings/tools";
  render(<App />);
  await screen.findByRole("region", { name: "Connectors" });
  read.mockRejectedValue(new Error("down"));
  await user.click(screen.getByRole("button", { name: "Check again" }));
  const alert = await screen.findByRole("alert");
  expect(within(alert).getByText(/Couldn’t check Claude Code tools/)).toBeInTheDocument();
  expect(within(alert).getByRole("button", { name: "Retry" })).toBeInTheDocument();
  expect(screen.getByRole("region", { name: "Connectors" })).toBeInTheDocument();
});
