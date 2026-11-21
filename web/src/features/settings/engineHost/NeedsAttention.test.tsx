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

it("draws nothing, and sends no request, while the service reports no engine host", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  render(<><NeedsAttentionGroup engines={engines} onOpen={vi.fn()} /><EngineGapCard engine={engines[0]!} /></>);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Needs attention")).toBeNull();
  expect(fetching).not.toHaveBeenCalled();
});

it("lists SDK missing, Unavailable and Expired with the one action each offers, and leaves Not found to the engine row", async () => {
  setEngineHostApi(createEngineHostMock({ hosts }));
  const onOpen = vi.fn();
  const user = userEvent.setup();
  render(<NeedsAttentionGroup engines={engines} onOpen={onOpen} />);
  const table = await screen.findByRole("table", { name: "Needs attention" });
  expect(screen.getByText("Engines that can’t run yet, and why. Each row says one thing and offers the one action that can change it.")).toBeVisible();
  const rowOf = (name: string) => within(within(table).getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement);
  expect(rowOf("Claude Code").getByText("This build does not include the Claude Code SDK. Claude Code runs through its claude program.")).toBeVisible();
  expect(rowOf("Claude Code").getByText("SDK missing")).toBeVisible();
  expect(rowOf("Cursor").getByText("The Cursor SDK is not included in this build. Cursor can’t run here.")).toBeVisible();
  expect(rowOf("Cursor").getByText("Unavailable")).toBeVisible();
  expect(rowOf("Cursor").queryByRole("button")).toBeNull();
  expect(rowOf("Codex").getByText("Your sign-in expired. Sign in with codex.")).toBeVisible();
  expect(within(table).queryByText("OpenCode", { selector: ".name" })).toBeNull();
  await user.click(rowOf("Claude Code").getByRole("button", { name: "Open" }));
  await user.click(rowOf("Codex").getByRole("button", { name: "Sign in again" }));
  expect(onOpen.mock.calls).toEqual([["claude"], ["codex"]]);
});

it("draws the card for each gap on a detail page", async () => {
  setEngineHostApi(createEngineHostMock({ hosts }));
  const { rerender } = render(<EngineGapCard engine={engines[0]!} />);
  expect(await screen.findByText("Claude Code SDK missing in this build")).toBeVisible();
  expect(screen.getByText("Claude Code runs through its claude program. Each run says “Running without the engine host”.")).toBeVisible();
  rerender(<EngineGapCard engine={engines[1]!} />);
  expect(await screen.findByText("Off for Cursor threads. Cursor can’t run here.")).toBeVisible();
  rerender(<EngineGapCard engine={engines[2]!} />);
  expect(await screen.findByText("Your sign-in expired. Sign in with codex.")).toBeVisible();
  expect(screen.getByText("Expired")).toBeVisible();
  rerender(<EngineGapCard engine={engines[3]!} />);
  expect(screen.queryByRole("group", { name: "Needs attention" })).toBeNull();
});
