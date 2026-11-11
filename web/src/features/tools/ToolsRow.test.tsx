import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { engineTools } from "../../../mock/fixtures";
import { api, ApiFailure } from "../../api/client";
import type { Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { ToolsRow } from "./ToolsRow";

const thread = (engine: string) => ({ id: `t-${engine}`, engine }) as Thread;

beforeEach(() => {
  useWorkspace.setState({
    engines: { data: [{ id: "claude", name: "Claude Code" }, { id: "codex", name: "Codex" }, { id: "opencode", name: "OpenCode" }] as never, error: null, loading: false },
  });
});
afterEach(() => vi.restoreAllMocks());

const row = () => screen.findByRole("button", { name: "Tools" });

it("counts what is ready, with the engine's name beneath and a chevron: it opens the Tools tab", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue({ ...engineTools("claude"), groups: engineTools("claude").groups.map((group) => ({ ...group, items: group.items.map((item) => ({ ...item, state: "ready" as const })) })) });
  const opened = vi.fn();
  const reopen = vi.spyOn(useUi.getState(), "reopenPanelTab");
  const user = userEvent.setup();
  const { container } = render(<ToolsRow thread={thread("claude")} onOpen={opened} />);
  const button = await row();
  await screen.findByText("10 ready");
  expect(button).toHaveTextContent("Claude Code");
  expect(container.querySelector(".dmain > svg:last-child")).not.toBeNull();
  await user.click(button);
  expect(reopen).toHaveBeenCalledWith("tools");
  expect(opened).toHaveBeenCalled();
});

it("puts a failure in red and a sign-in alone in amber on the second line, and a starting entry beside the count", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue(engineTools("claude"));
  const failing = render(<ToolsRow thread={thread("claude")} />);
  expect(await screen.findByText("1 needs sign-in · 1 failed")).toHaveClass("tg-card-bad");
  failing.unmount();
  vi.spyOn(api, "threadTools").mockResolvedValue(engineTools("codex"));
  render(<ToolsRow thread={thread("codex")} />);
  // Codex: one app starting counts as neither ready nor a problem, one sign-in is amber.
  expect(await screen.findByText("8 ready · 1 starting")).toBeInTheDocument();
  expect(screen.getByText("1 needs sign-in")).toHaveClass("amb");
});

it("says Not reported by the engine for one that lists no tools, in the tab's words", async () => {
  vi.spyOn(api, "threadTools").mockRejectedValue(new ApiFailure("unsupported", "This engine does not report its tools.", 422));
  render(<ToolsRow thread={thread("opencode")} />);
  expect(await screen.findByText("Not reported by OpenCode")).toBeInTheDocument();
  expect(screen.queryByText("Couldn’t read")).toBeNull();
  expect(screen.queryByText(/Shown once/)).toBeNull();
});

it("says Couldn’t read, never Not reported, when an engine that should have answered did not", async () => {
  vi.spyOn(api, "threadTools").mockRejectedValue(new ApiFailure("engine_failed", "OpenCode did not answer.", 500));
  render(<ToolsRow thread={thread("opencode")} />);
  expect(await screen.findByText("Couldn’t read")).toBeInTheDocument();
  expect(screen.queryByText(/Not reported by/)).toBeNull();
});

it("says Shown once this engine has run before a reporting engine has", async () => {
  vi.spyOn(api, "threadTools").mockResolvedValue({ ...engineTools("claude"), reported: false, groups: [], not_reported: [] });
  render(<ToolsRow thread={thread("claude")} />);
  expect(await screen.findByText("Shown once this engine has run")).toBeInTheDocument();
});
