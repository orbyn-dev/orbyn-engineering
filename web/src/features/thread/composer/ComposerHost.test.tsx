import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../../api/client";
import type { EngineModels, EngineStatus, Run, Thread } from "../../../api/types";
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
const stuck = (id: string, patch: Partial<HostRow> = {}): HostRow => ({ id, name: id, state: "cant_start", stops_in_window: 5, can_retry: true,
  last_error: { kind: "host", message: "the engine host closed its output", causes: [] }, ...patch });

beforeEach(() => {
  useUi.setState({ drafts: { t1: "Run the tests." } });
  useWorkspace.setState({ approvals: { data: [], error: null, loading: false } });
  useModelStore.setState({ byEngine: { claude: catalog("claude"), cursor: catalog("cursor") }, loading: {}, errors: {} });
  vi.spyOn(api, "queue").mockResolvedValue([]);
  vi.spyOn(api, "models").mockImplementation(async (id: string) => catalog(id));
});
afterEach(() => { setEngineHostApi(null); vi.restoreAllMocks(); });

const setup = (id: string, name: string) => {
  const engine = engineOf(id, name);
  useWorkspace.setState({ engines: { data: [engine], error: null, loading: false } });
  return render(<Composer thread={threadOf(id)} engine={engine} showContext />);
};

it("adds nothing while the service reports no engine host", () => {
  setup("claude", "Claude Code");
  expect(screen.queryByText(/can’t start|can’t send/)).toBeNull();
  expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
});

it("for Claude under the SDK path, the line gives the service's sentence with Use Claude (CLI) and Try again, and Send still works", async () => {
  const start = vi.spyOn(api, "startRun").mockResolvedValue({ id: "r1" } as Run);
  const reason = "The engine host is unavailable after repeated failures: the engine host closed its output";
  setEngineHostApi(createEngineHostMock({ hosts: [stuck("claude", { has_driver: true, detail: reason })] }));
  setup("claude", "Claude Code");
  expect(await screen.findByText(reason)).toBeVisible();
  expect(screen.getByRole("alert")).not.toHaveClass("info");
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Try again" })).toBeVisible();
  expect(screen.queryByText(/no other way to run here/)).toBeNull();
  expect(screen.queryByRole("button", { name: "Restart engine host" })).toBeNull();
  expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
  await userEvent.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(start).toHaveBeenCalledTimes(1));
});

it("with a wrapped driver, Send is still off, the message stays and the line offers Restart engine host", async () => {
  const start = vi.spyOn(api, "startRun").mockResolvedValue({ id: "r1" } as Run);
  const mock = createEngineHostMock({ delayMs: 20, hosts: [stuck("cursor", { has_driver: true })] });
  setEngineHostApi(mock);
  setup("cursor", "Cursor");
  expect(await screen.findByText("Cursor can’t send: its engine host is unavailable and Cursor has no other way to run here. Last error: the engine host closed its output.")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveAttribute("data-send", "off");
  expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  expect(screen.getByRole("textbox")).toHaveValue("Run the tests.");
  await userEvent.dblClick(screen.getByRole("button", { name: "Restart engine host" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:cursor"]));
  expect(start).not.toHaveBeenCalled();
});

it("for Claude under the SDK path, a restarting host says so with Use Claude (CLI) and no Try again", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [{ id: "claude", name: "Claude Code", state: "restarting", can_retry: false,
    detail: "The engine host stopped and is restarting." }] }));
  setup("claude", "Claude Code");
  expect(await screen.findByText("The engine host stopped and is restarting.")).toBeVisible();
  expect(screen.getByRole("alert")).toHaveClass("eh-line", "info");
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
});
