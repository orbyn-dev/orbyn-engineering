import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { request } from "../../../api/client";
import type { EngineStatus } from "../../../api/types";
import { ProviderDetail } from "../providers/ProviderDetail";
import { ProvidersSection } from "../providers/Providers";
import { setEngineHostApi } from "./hostApi";

const engines = vi.hoisted(() => ({ list: [] as unknown[] }));
vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));
vi.mock("../../../product/state/workspace", () => ({ useWorkspace: (select: (state: unknown) => unknown) => select({
  refreshEngines: vi.fn(), threads: {}, projects: { data: [] }, engines: { data: engines.list, error: null } }) }));
vi.mock("../../../product/state/connection", () => ({ useConnection: (select: (state: unknown) => unknown) => select({ offline: false }) }));
vi.mock("../rows/SettingsScopeContext", () => ({ useSettingsScope: () => null }));

const engine = (id: string, name: string): EngineStatus => ({ id, name, tier: "native", installed: true, path: `/bin/${id}`, version: "1.0.0", auth: "logged_in",
  account: "sam@example.com", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null, capabilities: {} } as unknown as EngineStatus);
const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: vi.fn(), reload: vi.fn() };

beforeEach(() => {
  engines.list = [engine("codex", "Codex"), engine("cursor", "Cursor")];
  vi.mocked(request).mockReset().mockImplementation(async (_method, path) => (path === "/api/providers" ? ({ providers: [] } as never) : String(path).includes("/installs") ? ({ outcomes: [] } as never) : ({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never)));
});
afterEach(() => setEngineHostApi(null));

it("shows neither group on the Providers page while the service reports no engine host data", async () => {
  render(<ProvidersSection />);
  expect(await screen.findByText("ACP registry")).toBeVisible();
  expect(screen.queryByText("Engine host")).toBeNull();
  expect(screen.queryByText("Sign-in storage")).toBeNull();
});

it("shows the Engine host and Sign-in storage groups at the end of the Providers page once the service reports them", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<ProvidersSection />);
  expect(await screen.findByRole("table", { name: "Engine host" })).toBeVisible();
  expect(await screen.findByRole("table", { name: "Sign-in storage" })).toBeVisible();
});

it("shows a provider's host banner, sign-in storage row and Engine host card on its detail page", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<ProviderDetail engine={engine("codex", "Codex")} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
  expect(await screen.findByRole("group", { name: "Engine host" })).toBeVisible();
  expect(screen.getAllByRole("alert").some((alert) => /Codex engine host stopped at/.test(alert.textContent ?? ""))).toBe(true);
  expect(await screen.findByText("Declined. Orbyn stores nothing; Codex keeps its own sign-in.")).toBeVisible();
});

it("changes nothing on a detail page while no host is reported", () => {
  render(<ProviderDetail engine={engine("codex", "Codex")} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
  expect(screen.queryByRole("group", { name: "Engine host" })).toBeNull();
  expect(screen.queryByText("Sign-in storage")).toBeNull();
});
