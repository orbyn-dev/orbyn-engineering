import { render, screen, waitFor } from "@testing-library/react";
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

const preference = (value: "sdk" | "cli", revision = `rev-${value}`): ScopedPreferencesResult => ({
  catalog: { version: "members-1", groups: [], metadata_complete: true, selection: { kind: "all_projects" } },
  rows: [{ key: "claude_engine_path", value, mixed: false, default: "sdk", project_scoped: false, revision,
    environment: { key: "claude_engine_path", value, default: "sdk", source: "global", project_scoped: false },
    members: [], overrides: [] }],
});

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });

function stubPreference(start: "sdk" | "cli") {
  let value = start;
  const writes: ScopedPreferencesResult["rows"][number]["value"][] = [];
  const fetching = vi.fn(async (input: string, init?: RequestInit) => {
    if (input === "/api/settings/preferences/query") return json(preference(value));
    if (input === "/api/settings/preferences" && init?.method === "PUT") {
      const body = JSON.parse(String(init.body)) as { edits: { claude_engine_path: { value: "sdk" | "cli" } } };
      value = body.edits.claude_engine_path.value;
      writes.push(value);
      return json(preference(value));
    }
    return json({ code: "not_found", message: "No mock." }, 404);
  });
  vi.stubGlobal("fetch", fetching);
  return { fetching, writes };
}

const host = (state: HostRow["state"]): HostRow => ({ id: "claude", name: "Claude Code", state, can_retry: false });

beforeEach(() => {
  useUi.setState({ drafts: { t1: "Run the tests." }, preferencesRevision: 0 });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false } });
  useModelStore.setState({ byEngine: { claude: catalog("claude") }, loading: {}, errors: {} });
  vi.spyOn(api, "queue").mockResolvedValue([]);
  vi.spyOn(api, "models").mockImplementation(async (id: string) => catalog(id));
});
afterEach(() => { setEngineHostApi(null); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

function setup(id: string, name: string) {
  const engine = engineOf(id, name);
  useWorkspace.setState({ engines: { data: [engine], error: null, loading: false } });
  return render(<Composer thread={threadOf(id)} engine={engine} showContext />);
}

it("offers the quiet switch to the SDK only when the person chose the CLI and the host is fine", async () => {
  const user = userEvent.setup();
  const { writes } = stubPreference("cli");
  setEngineHostApi(createEngineHostMock({ hosts: [host("running")] }));
  setup("claude", "Claude Code");
  const line = (await screen.findByText(/Claude \(CLI\) ·/)).parentElement!;
  expect(line).toHaveTextContent("Claude (CLI) · Switch to Claude (SDK)");
  await user.dblClick(screen.getByRole("button", { name: "Switch to Claude (SDK)" }));
  await waitFor(() => expect(writes).toEqual(["sdk"]));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Switch to Claude (SDK)" })).toBeNull());
  expect(screen.queryByRole("dialog")).toBeNull();
});

it("draws no line when the setting is already the SDK", async () => {
  stubPreference("sdk");
  setEngineHostApi(createEngineHostMock({ hosts: [host("running")] }));
  setup("claude", "Claude Code");
  await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledWith("/api/settings/preferences/query", expect.anything()));
  expect(screen.queryByRole("button", { name: "Switch to Claude (SDK)" })).toBeNull();
});

it.each(["cant_start", "restarting", "crashed", "version_mismatch"] as const)("draws no line while the engine host is %s", async (state) => {
  stubPreference("cli");
  setEngineHostApi(createEngineHostMock({ hosts: [host(state)] }));
  setup("claude", "Claude Code");
  await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledWith("/api/settings/preferences/query", expect.anything()));
  expect(screen.queryByRole("button", { name: "Switch to Claude (SDK)" })).toBeNull();
});

it("draws no line while no engine host is reported", async () => {
  stubPreference("cli");
  setEngineHostApi(createEngineHostMock({ hosts: null }));
  setup("claude", "Claude Code");
  await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledWith("/api/settings/preferences/query", expect.anything()));
  expect(screen.queryByRole("button", { name: "Switch to Claude (SDK)" })).toBeNull();
});

it("draws no line for another engine", async () => {
  stubPreference("cli");
  setEngineHostApi(createEngineHostMock({ hosts: [{ ...host("running"), id: "codex", name: "Codex" }] }));
  setup("codex", "Codex");
  expect(screen.queryByRole("button", { name: "Switch to Claude (SDK)" })).toBeNull();
  expect(vi.mocked(fetch)).not.toHaveBeenCalledWith("/api/settings/preferences/query", expect.anything());
});
