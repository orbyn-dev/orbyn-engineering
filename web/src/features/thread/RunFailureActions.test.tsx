import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { AgUiEvent, EngineStatus, Thread, WireThreadEvent } from "../../api/types";
import type { ScopedPreferencesResult } from "../../generated/ScopedPreferencesResult";
import { useThreadLogs } from "../../product/state/threadLogs";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { applyEvents, emptyLog } from "../../product/timeline/projection";
import { createEngineHostMock } from "../../../mock/engineHost";
import { setEngineHostApi } from "../enginehost";
import { ThreadItemView } from "./items";
import { TimelineThread } from "./timelineThread";

const HOST_STOPPED = "The engine host stopped. Your work in the project is kept; send your message again.";
const START_FAILED = "The engine host is unavailable: the engine host closed its output";

const wire = (seq: number, event: AgUiEvent): WireThreadEvent => ({
  seq, thread_id: "t", run_id: "r", ts: "2026-11-01T00:00:00Z", event,
});
const prompt = [
  wire(1, { type: "TEXT_MESSAGE_START", messageId: "u1", role: "user" }),
  wire(2, { type: "TEXT_MESSAGE_CONTENT", messageId: "u1", delta: "Find out why login sends people back to /home." }),
  wire(3, { type: "TEXT_MESSAGE_END", messageId: "u1" }),
];
const thread = (engine: string): Thread => ({ id: "t", project_id: "p", title: "Failed", engine, model: null, cwd: "/fixture",
  worktree: null, skill_profile: null, scratch: false, active_run: null, last_run_state: "failed", created_at: "", updated_at: "" });
const engineOf = (id: string) => ({ id, name: id === "claude" ? "Claude Code" : "Codex", installed: true, auth: "logged_in", capabilities: { managed: true } }) as EngineStatus;

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

function show(message: string, code: string | null, engine = "claude") {
  const log = applyEvents("t", emptyLog, [...prompt, wire(9, { type: "RUN_ERROR", message, code })]);
  const failure = log.items.findLast((item) => item.kind === "notice");
  if (!failure) throw new Error("no failure item");
  useThreadLogs.setState({ logs: { t: log } });
  useWorkspace.setState({
    threads: { t: thread(engine) },
    engines: { data: [engineOf(engine)], error: null, loading: false },
  });
  render(<TimelineThread value="t"><ThreadItemView item={failure} live={false} /></TimelineThread>);
}

function service(hostState: "cant_start" | "restarting" | "running" = "cant_start") {
  const mock = createEngineHostMock({ delayMs: 10, hosts: [{ id: "claude", name: "Claude Code", state: hostState, can_retry: hostState === "cant_start",
    detail: hostState === "cant_start" ? START_FAILED : null }] });
  setEngineHostApi(mock);
  return mock;
}

beforeEach(() => {
  useUi.setState({ preferencesRevision: 0 });
});
afterEach(() => {
  setEngineHostApi(null);
  vi.restoreAllMocks();
});

it("shows the host-stopped run's own sentence with Use Claude (CLI) and Try again, and Try again sends nothing", async () => {
  stubPreference("sdk");
  const mock = service("cant_start");
  const start = vi.spyOn(api, "startRun");
  show(HOST_STOPPED, "engine_host_stopped");
  expect(screen.getByText(HOST_STOPPED)).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:claude"]));
  expect(start).not.toHaveBeenCalled();
});

it("matches the stopped host by its code, not the sentence", async () => {
  stubPreference("sdk");
  service("cant_start");
  // The same words with another code are an ordinary failure and keep their code pill.
  show(HOST_STOPPED, "engine_failed");
  expect(screen.queryByRole("button", { name: "Use Claude (CLI)" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getByText("engine_failed")).toBeVisible();

  cleanup();
  show("The host is gone.", "engine_host_stopped");
  expect(screen.getByText("The host is gone.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
});

it("shows the start failure's own sentence with the same two actions", async () => {
  stubPreference("sdk");
  service("cant_start");
  show(START_FAILED, "engine_unavailable");
  expect(screen.getByText(START_FAILED)).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
});

it("Use Claude (CLI) writes once and the actions go away; the failed run stays", async () => {
  const { writes } = stubPreference("sdk");
  service("cant_start");
  show(HOST_STOPPED, "engine_host_stopped");
  await userEvent.dblClick(screen.getByRole("button", { name: "Use Claude (CLI)" }));
  await waitFor(() => expect(writes).toEqual(["cli"]));
  await waitFor(() => expect(screen.queryByRole("button", { name: "Use Claude (CLI)" })).toBeNull());
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getByText(HOST_STOPPED)).toBeVisible();
});

it("draws neither action while the path is the CLI", async () => {
  stubPreference("cli");
  service("cant_start");
  show(HOST_STOPPED, "engine_host_stopped");
  await waitFor(() => expect(vi.mocked(fetch)).toHaveBeenCalledWith("/api/settings/preferences/query", expect.anything()));
  expect(screen.queryByRole("button", { name: "Use Claude (CLI)" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getByText(HOST_STOPPED)).toBeVisible();
});

it("leaves another engine's host-stopped failure untouched", async () => {
  stubPreference("sdk");
  service("cant_start");
  show(HOST_STOPPED, "engine_host_stopped", "codex");
  expect(screen.queryByRole("button", { name: "Use Claude (CLI)" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getByText("engine_host_stopped")).toBeVisible();
});
