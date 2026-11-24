import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it } from "vitest";
import { createEngineHostMock } from "../../../../mock/engineHost";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { ProviderDetail } from "../providers/ProviderDetail";
import { ProvidersSection } from "../providers/Providers";
import { setEngineHostApi } from "../../enginehost";

const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: () => undefined, reload: () => undefined };
let mock: MockApi;
const engine = (id: string) => (mock.handle("GET", "/api/engines").body as EngineStatus[]).find((entry) => entry.id === id)!;

beforeEach(async () => {
  mock = installMockFetch();
  await useWorkspace.getState().refreshEngines();
});
afterEach(() => setEngineHostApi(null));

it("shows neither group on the Providers page while the service reports no engine host data", async () => {
  render(<ProvidersSection />);
  expect(await screen.findByText("ACP registry")).toBeVisible();
  expect(screen.queryByText("Engine host")).toBeNull();
  expect(screen.queryByText("API key storage")).toBeNull();
});

it("shows the Engine host and API key storage groups at the end of the Providers page once the service reports them", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<ProvidersSection />);
  expect(await screen.findByRole("table", { name: "Engine host" })).toBeVisible();
  expect(await screen.findByRole("table", { name: "API key storage" })).toBeVisible();
});

it("shows a provider's host banner, API key row and Engine host card on its detail page", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<ProviderDetail engine={engine("codex")} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
  expect(await screen.findByRole("group", { name: "Engine host" })).toBeVisible();
  expect(screen.getAllByRole("alert").some((alert) => /Codex engine host stopped at/.test(alert.textContent ?? ""))).toBe(true);
  expect(await screen.findByText("Orbyn stores nothing; Codex keeps its own sign-in.")).toBeVisible();
});

it("changes nothing on a detail page while no host is reported", () => {
  render(<ProviderDetail engine={engine("codex")} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
  expect(screen.queryByRole("group", { name: "Engine host" })).toBeNull();
  expect(screen.queryByText("API key storage")).toBeNull();
});

it("draws the provider-list banner, which is a note rather than a live status", async () => {
  render(<ProvidersSection />);
  const banner = await screen.findByText(/Orbyn lists an engine when it finds its program/);
  expect(banner.closest("[role=note]")).not.toBeNull();
  expect(banner.closest("[role=status]")).toBeNull();
});
