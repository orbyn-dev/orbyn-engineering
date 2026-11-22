import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { EngineModels, EngineStatus, Thread } from "../../../api/types";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";
import { useModelStore } from "../../../product/state/modelStore";
import { useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { setEngineHostApi, type HostRow } from "../../enginehost";
import { Composer } from "./Composer";

const engineOf = (id: string, name: string) => ({ id, name, installed: true, capabilities: { managed: true, permission_modes: ["supervised"], approvals: true, skill_delivery: [] } }) as unknown as EngineStatus;
const threadOf = (engine: string): Thread => ({ id: "t1", project_id: "p", title: "Thread", engine, model: "m", cwd: "/r", worktree: null, skill_profile: null,
  scratch: false, active_run: null, last_run_state: null, created_at: "", updated_at: "", lifecycle: { permission_mode: "supervised" } }) as unknown as Thread;
const catalog = (engine: string): EngineModels => ({ engine, source: "engine", checked_at: "fixture", error: null, fallback: false,
  models: [{ id: "m", label: "Model", description: null, is_default: true, custom: false, options: [] }], options: [], agents: [], output_styles: [] });
const host = (patch: Partial<HostRow>): HostRow => ({ id: "claude", name: "Claude Code", state: "cant_start", can_retry: true,
  last_error: { kind: "host", message: "the engine host closed its output", causes: [] }, ...patch });

const preference = (value: "sdk" | "cli"): ScopedPreferencesResult => ({
  catalog: { version: "members-1", groups: [], metadata_complete: true, selection: { kind: "all_projects" } },
  rows: [{ key: "claude_engine_path", value, mixed: false, default: "sdk", project_scoped: false, revision: `rev-${value}`,
    environment: { key: "claude_engine_path", value, default: "sdk", source: "global", project_scoped: false },
    members: [], overrides: [] }],
});
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function stubPreference(start: "sdk" | "cli") {
  let value = start;
  const writes: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    if (input === "/api/settings/preferences/query") return json(preference(value));
    if (input === "/api/settings/preferences" && init?.method === "PUT") {
      const body = JSON.parse(String(init.body)) as { edits: { claude_engine_path: { value: "sdk" | "cli" } } };
      value = body.edits.claude_engine_path.value;
      writes.push(value);
      return json(preference(value));
    }
    return json({ code: "not_found", message: "No mock." }, 404);
  }));
  return { writes };
}

beforeEach(() => {
  useUi.setState({ drafts: { t1: "Run the tests." }, preferencesRevision: 0 });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false } });
  useModelStore.setState({ byEngine: { claude: catalog("claude") }, loading: {}, errors: {} });
  vi.spyOn(api, "queue").mockResolvedValue([]);
  vi.spyOn(api, "models").mockImplementation(async (id: string) => catalog(id));
});
afterEach(() => {
  cleanup();
  setEngineHostApi(null);
  useWorkspace.setState({ updates: { data: null, error: null, loading: false } });
  vi.restoreAllMocks();
});

function setup() {
  const engine = engineOf("claude", "Claude Code");
  useWorkspace.setState({ engines: { data: [engine], error: null, loading: false } });
  render(<Composer thread={threadOf("claude")} engine={engine} showContext />);
}

it("Use Claude (CLI) writes the app-only setting once and the host line goes away", async () => {
  const { writes } = stubPreference("sdk");
  setEngineHostApi(createEngineHostMock({ delayMs: 10, hosts: [host({ detail: "The engine host is unavailable after repeated failures: the engine host closed its output" })] }));
  setup();
  const use = await screen.findByRole("button", { name: "Use Claude (CLI)" });
  await userEvent.dblClick(use);
  await waitFor(() => expect(writes).toEqual(["cli"]));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Use Claude (CLI)" })).toBeNull());
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getByRole("textbox")).toHaveValue("Run the tests.");
});

it("draws none of the host-down lines while the path is the CLI", async () => {
  stubPreference("cli");
  setEngineHostApi(createEngineHostMock({ hosts: [host({ detail: "The engine host is unavailable after repeated failures: the engine host closed its output" })] }));
  setup();
  await waitFor(() => {
    expect(screen.queryByRole("button", { name: "Use Claude (CLI)" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
    expect(screen.queryByText(/engine host is unavailable/)).toBeNull();
  });
  expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
});

it("shows the mismatch sentence with Update Orbyn when an update is ready, and Use Claude (CLI)", async () => {
  stubPreference("sdk");
  vi.spyOn(api, "applyAppUpdate").mockResolvedValue(undefined as never);
  useWorkspace.setState({ updates: { data: { app: { phase: "ready", can_apply: true, release_id: "release-1", operation: 3 } } as never, error: null, loading: false } });
  setEngineHostApi(createEngineHostMock({ hosts: [host({ state: "version_mismatch", can_retry: false, protocol: 2, app_protocol: 1,
    detail: "The engine host does not match this app version (app protocol 1, host protocol 2). Update Orbyn." })] }));
  setup();
  expect(await screen.findByText("The engine host does not match this app version (app protocol 1, host protocol 2). Update Orbyn.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Update Orbyn" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
});
