import { act, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createEngineHostMock } from "../../../../mock/engineHost";
import type { EngineStatus } from "../../../api/types";
import { setEngineHostApi, type HostRow } from "../../enginehost";
import { EngineGapCard, NeedsAttentionGroup } from "./NeedsAttention";

afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); });
const engine = (id: string, name: string, patch: Partial<EngineStatus> = {}) => ({ id, name, installed: true, ...patch }) as EngineStatus;
const host = (id: string, patch: Partial<HostRow>): HostRow => ({ id, name: id, state: "running", can_retry: false, ...patch });
const engines = [
  engine("claude", "Claude Code"), engine("cursor", "Cursor"), engine("codex", "Codex"), engine("opencode", "OpenCode", { installed: false }),
];
const hosts = [host("claude", { sdk_missing: true, has_driver: true }), host("cursor", { sdk_missing: true }), host("codex", { last_error: { kind: "auth", message: "401", causes: [] } })];

it("draws nothing while the service answers 404 for the engine host", async () => {
  const fetching = vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404, headers: { "content-type": "application/json" } }));
  vi.stubGlobal("fetch", fetching);
  render(<><NeedsAttentionGroup engines={engines} onOpen={vi.fn()} /><EngineGapCard engine={engines[0]!} /></>);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Needs attention")).toBeNull();
  expect(fetching).toHaveBeenCalled();
});

it("lists SDK missing and Expired with the one action each offers, and leaves Not found to the engine row", async () => {
  setEngineHostApi(createEngineHostMock({ hosts }));
  const onOpen = vi.fn();
  const user = userEvent.setup();
  render(<NeedsAttentionGroup engines={engines} onOpen={onOpen} />);
  const table = await screen.findByRole("table", { name: "Needs attention" });
  expect(screen.getByText("Engines that can’t run yet, and why. Each row says one thing and offers the one action that can change it.")).toBeVisible();
  const rowOf = (name: string) => within(within(table).getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement);
  expect(rowOf("Claude Code").getByText("This build does not include the Claude Code SDK, so a run on Claude Code fails with that message. Choose Claude (CLI) in Claude path to run it.")).toBeVisible();
  expect(rowOf("Claude Code").getByText("SDK missing")).toBeVisible();
  expect(rowOf("Cursor").getByText("This build does not include the Cursor SDK, so a run on Cursor fails with that message.")).toBeVisible();
  expect(rowOf("Cursor").getByText("SDK missing")).toBeVisible();
  expect(rowOf("Cursor").queryByText("Unavailable")).toBeNull();
  expect(rowOf("Cursor").queryByRole("button")).toBeNull();
  expect(rowOf("Codex").getByText("Your sign-in expired. Sign in with codex.")).toBeVisible();
  expect(within(table).queryByText("OpenCode", { selector: ".name" })).toBeNull();
  await user.click(rowOf("Claude Code").getByRole("button", { name: "Open" }));
  await user.click(rowOf("Codex").getByRole("button", { name: "Sign in again" }));
  expect(onOpen.mock.calls).toEqual([["claude"], ["codex"]]);
});

it("keeps the row but offers no action that would open a page the beta has not built, for an engine outside it", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host("cursor", { last_error: { kind: "auth", message: "401", causes: [] } })] }));
  const onOpen = vi.fn();
  render(<NeedsAttentionGroup engines={[engine("cursor", "Cursor")]} onOpen={onOpen} />);
  const table = await screen.findByRole("table", { name: "Needs attention" });
  const row = within(within(table).getByText("Cursor", { selector: ".name" }).closest("[role=row]") as HTMLElement);
  // Cursor’s page is out of the beta, so the row says what happened and offers nothing to press.
  expect(row.getByText("Your sign-in expired. Sign in with cursor.")).toBeVisible();
  expect(row.getByText("Expired")).toBeVisible();
  expect(row.queryByRole("button")).toBeNull();
  expect(onOpen).not.toHaveBeenCalled();
});

it("draws the card for each gap on a detail page", async () => {
  setEngineHostApi(createEngineHostMock({ hosts }));
  const { rerender } = render(<EngineGapCard engine={engines[0]!} />);
  expect(await screen.findByText("Claude Code SDK missing in this build")).toBeVisible();
  expect(screen.getByText("What happens")).toBeVisible();
  expect(screen.getByText("A run fails with “This build does not include the engine's SDK.” Claude (CLI) in Claude path still runs it.")).toBeVisible();
  expect(screen.queryByText("What runs")).toBeNull();
  rerender(<EngineGapCard engine={engines[1]!} />);
  expect(await screen.findByText("A run fails with “This build does not include the engine's SDK.”")).toBeVisible();
  expect(screen.queryByText(/Claude \(CLI\) in Claude path still runs it/)).toBeNull();
  expect(screen.queryByText(/Off for Cursor threads/)).toBeNull();
  rerender(<EngineGapCard engine={engines[2]!} />);
  expect(await screen.findByText("Your sign-in expired. Sign in with codex.")).toBeVisible();
  expect(screen.getByText("Expired")).toBeVisible();
  rerender(<EngineGapCard engine={engines[3]!} />);
  expect(screen.queryByRole("group", { name: "Needs attention" })).toBeNull();
});
