import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import type { EngineStatus } from "../../../api/types";
import { api } from "../../../api/client";
import { useWorkspace } from "../../../product/state/workspace";
import type { HostRow } from "../../enginehost";
import { CLAUDE_PATH_HELP } from "./ClaudePath";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { setEngineHostApi } from "../../enginehost";
import { installMockFetch } from "../../../test-support";
import { ProviderDetail } from "./ProviderDetail";

const engine = { id: "claude", name: "Claude Code", tier: "native", installed: true, version: "2.1.318", latest: null, auth: "logged_in", path: "/opt/claude",
  capabilities: {}, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null } as unknown as EngineStatus;
const store = { instances: [], raw: [], disabled: false, save: async () => null } as never;
const sdk = () => screen.getByRole("radio", { name: /Claude \(SDK\)/ });
const cli = () => screen.getByRole("radio", { name: /Claude \(CLI\)/ });
const putBodies = () => vi.mocked(fetch).mock.calls
  .filter(([input, init]) => input === "/api/settings/preferences" && init?.method === "PUT")
  .map(([, init]) => JSON.parse(String(init?.body)));

function host(patch: Partial<HostRow>): HostRow {
  return { id: "claude", name: "Claude Code", state: "running", can_retry: false, ...patch };
}

async function show(hostRow: HostRow = host({})) {
  const mock = installMockFetch();
  const hosts = createEngineHostMock({ hosts: [hostRow] });
  setEngineHostApi(hosts);
  render(<ProviderDetail engine={engine} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  await screen.findByRole("radio", { name: /Claude \(SDK\)/ });
  return { mock, hosts };
}

afterEach(() => {
  setEngineHostApi(null);
  useWorkspace.setState({ updates: { data: null, error: null, loading: false } });
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

it("puts the Claude path under Location with the help line and Default on the SDK choice", async () => {
  await show();
  expect(screen.getByText("Claude path")).toBeVisible();
  expect(screen.getByText(CLAUDE_PATH_HELP)).toBeVisible();
  expect(sdk()).toHaveAttribute("aria-checked", "true");
  expect(cli()).toHaveAttribute("aria-checked", "false");
  expect(screen.getByText("Default")).toBeVisible();
  expect(screen.getByText("Engine host")).toBeVisible();
  expect(screen.getByText("Used by Claude (SDK)")).toBeVisible();
});

it("writes claude_engine_path once with the read revision, applies to the next message, and needs no restart", async () => {
  const user = userEvent.setup();
  await show();
  await user.click(cli());
  expect(await screen.findByText("Next message uses Claude (CLI).")).toBeVisible();
  expect(await screen.findByText("Not used")).toBeVisible();
  expect(cli()).toHaveAttribute("aria-checked", "true");
  expect(screen.queryByRole("dialog")).toBeNull();
  await waitFor(() => expect(putBodies()).toHaveLength(1));
  expect(putBodies()[0]).toMatchObject({
    scope: {},
    membership_version: expect.any(String),
    if_match: { claude_engine_path: expect.any(String) },
    edits: { claude_engine_path: { op: "set", value: "cli" } },
  });
});

it("does not write when the chosen path is clicked again", async () => {
  const user = userEvent.setup();
  await show();
  await user.click(sdk());
  expect(putBodies()).toHaveLength(0);
});

it("writes once when the choice is clicked twice", async () => {
  const user = userEvent.setup();
  await show();
  await user.dblClick(cli());
  expect(await screen.findByText("Next message uses Claude (CLI).")).toBeVisible();
  await waitFor(() => expect(putBodies()).toHaveLength(1));
  expect(putBodies()[0]).toMatchObject({ edits: { claude_engine_path: { op: "set", value: "cli" } } });
});

it("says nothing changed and keeps the old value when the write fails", async () => {
  const user = userEvent.setup();
  await show();
  const inner = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    if (input === "/api/settings/preferences" && init?.method === "PUT") {
      return new Response(JSON.stringify({ code: "settings_conflict", message: "Settings changed. Reload before saving." }), { status: 409 });
    }
    return inner(input, init);
  });
  await user.click(cli());
  expect(await screen.findByText("Nothing changed. Settings changed. Reload before saving.")).toBeVisible();
  expect(sdk()).toHaveAttribute("aria-checked", "true");
  expect(cli()).toHaveAttribute("aria-checked", "false");
  expect(screen.queryByText("Next message uses Claude (CLI).")).toBeNull();
  expect(screen.getByText("Running")).toBeVisible();
  expect(putBodies()).toHaveLength(1);
});

it("shows the host's own sentence and the CLI way out while restarting, with no Try again", async () => {
  await show(host({ state: "restarting", detail: "The engine host stopped and is restarting." }));
  const banner = screen.getByText("The engine host stopped and is restarting.");
  expect(banner).toBeVisible();
  expect(banner.closest("[role=status]")).not.toBeNull();
  expect(screen.getByText("Restarting")).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
  expect(sdk()).toHaveAttribute("aria-checked", "true");
});

it("falls back to the built host sentence when an older service sends no detail", async () => {
  await show(host({ state: "restarting" }));
  expect(screen.getByText("The engine host stopped and is restarting.")).toBeVisible();
  expect(screen.getByText("Restarting")).toBeVisible();
});

it("falls back to the protocol sentence when a mismatch carries no detail", async () => {
  await show(host({ state: "version_mismatch", protocol: 2, app_protocol: 1 }));
  expect(screen.getByText("The engine host does not match this app version. Host protocol 2, app protocol 1.")).toBeVisible();
  expect(screen.getByText("Version mismatch")).toBeVisible();
});

it("does not write when the chosen path is already selected", async () => {
  const user = userEvent.setup();
  await show();
  await user.click(sdk());
  await waitFor(() => expect(sdk()).toHaveAttribute("aria-checked", "true"));
  expect(putBodies()).toHaveLength(0);
});

it("shows the unavailable reason with Use Claude (CLI) and Try again", async () => {
  const user = userEvent.setup();
  const reason = "The engine host is unavailable after repeated failures: the engine host closed its output";
  const { hosts } = await show(host({ state: "cant_start", can_retry: true, detail: reason,
    last_error: { message: "The engine host closed its output", causes: [] } }));
  const banner = screen.getByText(reason);
  expect(banner).toBeVisible();
  expect(banner.closest("[role=alert]")).not.toBeNull();
  expect(screen.getByText("Can’t start")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(hosts.writes).toEqual(["retry:claude"]));
  await waitFor(() => expect(screen.queryByText(reason)).toBeNull());
  expect(sdk()).toHaveAttribute("aria-checked", "true");
});

it("shows the mismatch sentence with Use Claude (CLI), and offers Update Orbyn only when an update is ready", async () => {
  await show(host({ state: "version_mismatch", protocol: 2, app_protocol: 1,
    detail: "The engine host does not match this app version (app protocol 1, host protocol 2). Update Orbyn." }));
  expect(screen.getByText("The engine host does not match this app version (app protocol 1, host protocol 2). Update Orbyn.")).toBeVisible();
  expect(screen.getByText("Version mismatch")).toBeVisible();
  expect(screen.getByRole("button", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Update Orbyn" })).toBeNull();
});

it("offers Update Orbyn for a mismatch when the app has an update ready", async () => {
  const user = userEvent.setup();
  const apply = vi.spyOn(api, "applyAppUpdate").mockResolvedValue(undefined as never);
  useWorkspace.setState({ updates: { data: { app: { phase: "ready", can_apply: true, release_id: "release-1", operation: 3 } } as never, error: null, loading: false } });
  await show(host({ state: "version_mismatch", protocol: 2, app_protocol: 1, detail: "Update Orbyn." }));
  await user.click(screen.getByRole("button", { name: "Update Orbyn" }));
  await waitFor(() => expect(apply).toHaveBeenCalledWith({ release_id: "release-1", operation: 3 }));
});
