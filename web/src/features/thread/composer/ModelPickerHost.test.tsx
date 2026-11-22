import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { EngineModels, EngineStatus } from "../../../api/types";
import { useModelStore } from "../../../product/state/modelStore";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { setEngineHostApi, type HostRow } from "../../enginehost";
import { ModelPickerTrigger } from "./ModelPicker";

vi.mock("../../../api/client", async (original) => {
  const actual = await original<typeof import("../../../api/client")>();
  return { ...actual, api: { ...actual.api, models: vi.fn() } };
});

const engine = (id: string, name: string, patch: Partial<EngineStatus> = {}) => ({ id, name, installed: true, capabilities: { managed: true }, ...patch }) as EngineStatus;
const engines = [engine("claude", "Claude Code"), engine("codex", "Codex"), engine("cursor", "Cursor"), engine("opencode", "OpenCode"), engine("pi", "Pi")];
const models = (id: string): EngineModels => ({ engine: id, source: "engine", checked_at: "fixture", error: null, fallback: false,
  models: [{ id: `${id}-m`, label: `${id} model`, description: null, is_default: true, custom: false, options: [] }], options: [], agents: [], output_styles: [] });
const row = (id: string, patch: Partial<HostRow>): HostRow => ({ id, name: id, state: "running", can_retry: false, ...patch });

beforeEach(() => {
  vi.mocked(api.models).mockImplementation(async (id: string) => models(id));
  useModelStore.setState({ byEngine: Object.fromEntries(engines.map((e) => [e.id, models(e.id)])), loading: {}, errors: {} });
});
afterEach(() => setEngineHostApi(null));

async function open() {
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={engines} selected={{ engine: "claude", model: "claude-m" }} onPick={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "claude model" }));
  return { user, picker: await screen.findByRole("dialog", { name: "Choose a model" }) };
}

it("shows only models while the service reports no engine host", async () => {
  const { picker } = await open();
  expect(within(picker).getAllByRole("option")).toHaveLength(6);
  expect(within(picker).queryByText("Starting the engine host")).toBeNull();
});

it("puts one state row in place of the models of an engine whose host is not ready, in the same slot", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [
    row("claude", {}),
    row("codex", { state: "starting" }),
    row("cursor", { state: "restarting", restart: { attempt: 3, limit: 5, next_try_seconds: 4 } }),
    row("opencode", { state: "cant_start", stops_in_window: 5, can_retry: true, last_error: { kind: "host", message: "the engine host closed its output", causes: [] } }),
    row("pi", { state: "version_mismatch" }),
  ] }));
  const { picker } = await open();
  await within(picker).findByText("Starting the engine host");
  expect(within(picker).getByText("Its models appear when it is ready. It has 10 seconds; the first start after an update has 30.")).toBeVisible();
  expect(within(picker).getByText("Restart 3 of 5 · next try in 4 s. Your work in the project is kept.")).toBeVisible();
  expect(within(picker).getByText("The engine host stopped 5 times in 5 minutes. Last error: the engine host closed its output.")).toBeVisible();
  expect(within(picker).getByText("The engine host does not match this app version.")).toBeVisible();
  // The running family keeps its models on both paths; the others offer none, so nothing can be picked from a state row.
  expect(within(picker).getAllByRole("option")).toHaveLength(2);
  expect(within(picker).getAllByRole("group")).toHaveLength(6);
  expect(within(picker).getByRole("button", { name: "Try again" })).toBeVisible();
  // No app update is ready here, so Update Orbyn is not offered and nothing dead is drawn.
  expect(within(picker).queryByRole("button", { name: "Update Orbyn" })).toBeNull();
});

it("Try again is one write, however fast the clicks", async () => {
  const mock = createEngineHostMock({ delayMs: 20, hosts: [row("opencode", { state: "cant_start", stops_in_window: 5, can_retry: true })] });
  setEngineHostApi(mock);
  const { user, picker } = await open();
  await user.dblClick(await within(picker).findByRole("button", { name: "Try again" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:opencode"]));
});

it("names SDK missing, not installed and sign in again; the CLI path still offers its model", async () => {
  const list = [engine("claude", "Claude Code"), engine("cursor", "Cursor", { installed: false }), engine("muse", "Muse Code")];
  useModelStore.setState({ byEngine: Object.fromEntries(list.map((e) => [e.id, models(e.id)])), loading: {}, errors: {} });
  setEngineHostApi(createEngineHostMock({ hosts: [row("claude", { sdk_missing: true, has_driver: true }), row("muse", { last_error: { kind: "auth", message: "401", causes: [] } })] }));
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={list} selected={{ engine: "claude", model: "claude-m" }} onPick={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "claude model" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  expect(await within(picker).findByText("This build does not include the Claude Code SDK, so a run on Claude Code fails with that message. Choose Claude (CLI) in Claude path to run it.")).toBeVisible();
  expect(within(picker).getByText("cursor isn’t installed on this computer.")).toBeVisible();
  expect(within(picker).getAllByText("Not found").length).toBeGreaterThan(0);
  expect(within(picker).getByText("Your sign-in expired. Sign in with muse.")).toBeVisible();
  expect(within(picker).getByRole("button", { name: "Sign in again" })).toBeVisible();
  // The SDK row has no models, but the CLI path still runs the same installed claude.
  expect(within(picker).getAllByRole("option")).toHaveLength(1);
  expect(within(picker).getByRole("group", { name: "Claude (CLI)" })).toHaveTextContent("claude model");
});

it("does not show a state row while searching", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [row("codex", { state: "starting" })] }));
  const { user, picker } = await open();
  await within(picker).findByText("Starting the engine host");
  await user.type(within(picker).getByRole("combobox", { name: "Search models" }), "claude");
  expect(within(picker).queryByText("Starting the engine host")).toBeNull();
});
