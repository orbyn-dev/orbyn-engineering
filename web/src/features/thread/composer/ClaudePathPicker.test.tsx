import { render, screen, waitFor, within } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { EngineModels, EngineStatus } from "../../../api/types";
import type { HostRow } from "../../enginehost";
import { setEngineHostApi } from "../../enginehost";
import { useModelStore } from "../../../product/state/modelStore";
import { useWorkspace } from "../../../product/state/workspace";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { installMockFetch } from "../../../test-support";
import { wire } from "../../../wire.test-support";
import { CLAUDE_CLI_NOTE, CLAUDE_SDK_NOTE } from "../../settings/providers/ClaudePathRows";
import { ModelPickerTrigger } from "./ModelPicker";

const engine = (id: string, name: string, patch: Partial<EngineStatus> = {}) =>
  ({ id, name, installed: true, auth: "logged_in", capabilities: { managed: true }, ...patch }) as EngineStatus;
const catalog = (engineId: string): EngineModels => ({
  engine: engineId, source: "engine", checked_at: "fixture", error: null, fallback: false,
  models: [{ id: engineId === "claude" ? "opus" : "small", label: engineId === "claude" ? "Opus 5.5" : "Small model",
    description: null, is_default: true, custom: false, options: [] }],
  options: [], agents: [], output_styles: [],
});
const claudeCatalog: EngineModels = {
  ...catalog("claude"),
  models: [
    { id: "opus", label: "Opus 5.5", description: "Deepest reasoning.", is_default: true, custom: false, options: [] },
    { id: "sonnet", label: "Sonnet 5.5", description: null, is_default: false, custom: false, options: [] },
  ],
};
const codexCatalog = catalog("codex");
const engines = () => [engine("claude", "Claude Code"), engine("codex", "Codex")];
const host = (patch: Partial<HostRow>): HostRow => ({ id: "claude", name: "Claude Code", state: "running", can_retry: false, ...patch });

let mock: ReturnType<typeof installMockFetch>;
const putBodies = () => wire().filter((call) => call.method === "PUT" && call.path === "/api/settings/preferences").map((call) => call.body);

async function open(selected = { engine: "claude", model: "opus" }, onPick = vi.fn()) {
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={engines()} selected={selected} onPick={onPick} />);
  await user.click(screen.getByRole("button", { name: selected.model === "opus" ? "Opus 5.5" : "Small model" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  return { user, picker, onPick };
}

const group = (picker: HTMLElement, name: string) => within(picker).getByRole("group", { name });

beforeEach(() => {
  mock = installMockFetch();
  mock.setEngineModels("claude", claudeCatalog);
  mock.setEngineModels("codex", codexCatalog);
  useModelStore.setState({ byEngine: { claude: claudeCatalog, codex: codexCatalog }, loading: {}, errors: {} });
});
afterEach(() => {
  setEngineHostApi(null);
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("draws Claude (SDK) and Claude (CLI) with their one helper line, and the selected path's model is marked", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({})] }));
  const { picker } = await open();
  expect(within(picker).getAllByRole("button").filter((button) => button.hasAttribute("aria-pressed")).map((tab) => tab.textContent))
    .toEqual(["All", "Claude (SDK)", "Claude (CLI)", "Codex"]);
  expect(within(group(picker, "Claude (SDK)")).getByText(CLAUDE_SDK_NOTE)).toBeVisible();
  expect(within(group(picker, "Claude (CLI)")).getByText(CLAUDE_CLI_NOTE)).toBeVisible();
  expect(within(group(picker, "Claude (SDK)")).getByRole("option", { name: /Opus 5.5/ })).toHaveAttribute("aria-selected", "true");
  expect(within(group(picker, "Claude (CLI)")).getByRole("option", { name: /Opus 5.5/ })).toHaveAttribute("aria-selected", "false");
  // Both rows are the same installed claude.
  expect(within(group(picker, "Claude (SDK)")).getByText("Ready")).toBeVisible();
  expect(within(group(picker, "Claude (CLI)")).getByText("Ready")).toBeVisible();
  // The SDK row carries the host chip; the CLI row does not.
  expect(within(group(picker, "Claude (SDK)")).getByText("Running")).toBeVisible();
  expect(within(group(picker, "Claude (CLI)")).queryByText("Running")).toBeNull();
});

it("picking the CLI row writes claude_engine_path once, with no dialog, and moves the mark", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({})] }));
  const { user, picker } = await open();
  await user.click(within(picker).getByRole("button", { name: "Claude (CLI) path" }));
  await waitFor(() => expect(putBodies()).toHaveLength(1));
  expect(putBodies()[0]).toMatchObject({ edits: { claude_engine_path: { op: "set", value: "cli" } } });
  await waitFor(() => expect(within(group(picker, "Claude (CLI)")).getByRole("option", { name: /Opus 5.5/ })).toHaveAttribute("aria-selected", "true"));
  expect(within(group(picker, "Claude (SDK)")).getByRole("option", { name: /Opus 5.5/ })).toHaveAttribute("aria-selected", "false");
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
});

it("says nothing changed and keeps the old path when the write fails", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({})] }));
  const { user, picker } = await open();
  mock.fail("/api/settings/preferences", 1, 409, "settings_conflict", "Settings changed. Reload before saving.");
  await user.click(within(picker).getByRole("button", { name: "Claude (CLI) path" }));
  expect(await within(picker).findByText("Nothing changed. Settings changed. Reload before saving.")).toBeVisible();
  expect(within(group(picker, "Claude (SDK)")).getByRole("option", { name: /Opus 5.5/ })).toHaveAttribute("aria-selected", "true");
  expect(putBodies()).toHaveLength(1);
});

it("writes once when the path row is clicked twice", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({})] }));
  const { user, picker } = await open();
  const row = within(picker).getByRole("button", { name: "Claude (CLI) path" });
  await user.dblClick(row);
  await waitFor(() => expect(putBodies()).toHaveLength(1));
});

it("draws the path picker at the frame's 480 px, and 320 px in a narrow window", () => {
  const css = readFileSync(join(process.cwd(), "src/styles/model-picker.css"), "utf8");
  expect(css).toMatch(/\.model-picker\.has-claude-paths \{[^}]*width: min\(480px, calc\(100vw - 12px\)\)/);
  expect(css).toMatch(/@media \(max-width: 640px\)[\s\S]*\.model-picker\.has-claude-paths \{[^}]*width: min\(320px, calc\(100vw - 12px\)\)/);
});

it("picks a model on the CLI row while the SDK host is down, writing the path once", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({ state: "cant_start", can_retry: true, detail: "The engine host is unavailable." })] }));
  const pick = vi.fn();
  const { user, picker } = await open(undefined, pick);
  await user.click(within(group(picker, "Claude (CLI)")).getByRole("option", { name: /Sonnet 5.5/ }));
  await waitFor(() => expect(putBodies()).toHaveLength(1));
  expect(putBodies()[0]).toMatchObject({ edits: { claude_engine_path: { op: "set", value: "cli" } } });
  expect(pick).toHaveBeenCalledWith({ engine: "claude", model: "sonnet" });
});

it("shows Not found in the same words on both rows, with the sentence and Check again in place of the models", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({})] }));
  const user = userEvent.setup();
  const list = [engine("claude", "Claude Code", { installed: false }), engine("codex", "Codex")];
  render(<ModelPickerTrigger engines={list} selected={{ engine: "claude", model: "opus" }} onPick={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Opus 5.5" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  for (const name of ["Claude (SDK)", "Claude (CLI)"]) {
    const row = group(picker, name);
    expect(within(row).getByText("Not found")).toBeVisible();
    expect(within(row).getByText("Not installed")).toBeVisible();
    expect(within(row).getByText("claude isn’t installed on this computer.")).toBeVisible();
    expect(within(row).getByRole("button", { name: "Check again" })).toBeVisible();
    expect(within(row).queryAllByRole("option")).toHaveLength(0);
  }
  await user.click(within(group(picker, "Claude (SDK)")).getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(wire().filter((call) => call.method === "POST" && call.path === "/api/engines/refresh")).toHaveLength(1));
});

it("shows Signed out in the same words on both rows, with no action", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({})] }));
  const user = userEvent.setup();
  render(<ModelPickerTrigger engines={[engine("claude", "Claude Code", { auth: "logged_out" })]} selected={{ engine: "claude", model: "opus" }} onPick={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "Opus 5.5" }));
  const picker = await screen.findByRole("dialog", { name: "Choose a model" });
  for (const name of ["Claude (SDK)", "Claude (CLI)"]) {
    const row = group(picker, name);
    expect(within(row).getAllByText("Signed out").length).toBeGreaterThan(0);
    expect(within(row).getByText("Sign in to Claude Code in a terminal, then try again.")).toBeVisible();
    expect(within(row).queryByRole("button", { name: "Check again" })).toBeNull();
  }
});

it("shows the service's sentence with Use Claude (CLI) and Try again for a host that can't start, and the CLI row stays pickable", async () => {
  const reason = "The engine host is unavailable after repeated failures: the engine host closed its output";
  const mock = createEngineHostMock({ delayMs: 10, hosts: [host({ state: "cant_start", can_retry: true, detail: reason,
    last_error: { message: "The engine host closed its output", causes: [] } })] });
  setEngineHostApi(mock);
  const { user, picker } = await open();
  const sdk = group(picker, "Claude (SDK)");
  expect(within(sdk).getByText(reason)).toBeVisible();
  expect(within(sdk).getAllByText("Can’t start")).toHaveLength(2);
  expect(within(sdk).getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(within(sdk).getByRole("button", { name: "Try again" })).toBeVisible();
  // The CLI path is unaffected and can be picked below.
  expect(within(group(picker, "Claude (CLI)")).getAllByRole("option")).toHaveLength(2);
  await user.click(within(sdk).getByRole("button", { name: "Use Claude (CLI)" }));
  await waitFor(() => expect(putBodies()).toEqual([expect.objectContaining({ edits: { claude_engine_path: { op: "set", value: "cli" } } })]));
  await user.dblClick(within(sdk).getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:claude"]));
});

it("shows Use Claude (CLI) only while the host is restarting", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [host({ state: "restarting", detail: "The engine host stopped and is restarting." })] }));
  const { picker } = await open();
  expect(within(picker).getByText("The engine host stopped and is restarting.")).toBeVisible();
  expect(within(picker).getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(within(picker).queryByRole("button", { name: "Try again" })).toBeNull();
});

it("shows the mismatch sentence with Update Orbyn when an update is ready and Use Claude (CLI)", async () => {
  useWorkspace.setState({ updates: { data: { app: { phase: "ready", can_apply: true, release_id: "release-1", operation: 3 } } as never, error: null, loading: false } });
  setEngineHostApi(createEngineHostMock({ hosts: [host({ state: "version_mismatch", protocol: 2, app_protocol: 1,
    detail: "The engine host does not match this app version (app protocol 1, host protocol 2). Update Orbyn." })] }));
  const { user, picker } = await open();
  expect(within(picker).getByText("The engine host does not match this app version (app protocol 1, host protocol 2). Update Orbyn.")).toBeVisible();
  expect(within(picker).getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  await user.click(within(picker).getByRole("button", { name: "Update Orbyn" }));
  await waitFor(() => expect(wire().filter((call) => call.method === "POST" && call.path.startsWith("/api/updates"))).toEqual([expect.objectContaining({ body: { release_id: "release-1", operation: 3 } })]));
});
