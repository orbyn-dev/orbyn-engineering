import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { ApiFailure, request } from "../../../api/client";
import { ProvidersSection } from "./Providers";

const mock = vi.hoisted(() => ({ refresh: vi.fn(), check: vi.fn(), offline: false }));
vi.mock("../../../api/client", async (original) => {
  const actual = await original<typeof import("../../../api/client")>();
  return { ...actual, request: vi.fn(), api: { ...actual.api, refreshEngines: mock.check } };
});
vi.mock("../../../product/state/workspace", () => ({ useWorkspace: (select: (state: unknown) => unknown) => select({
  refreshEngines: mock.refresh, engines: { data: [{ id: "test", name: "Test engine", tier: "acp", installed: true, auth: "not_applicable", version: "1", capabilities: {} }], error: null },
}) }));
vi.mock("../../../product/state/connection", () => ({ useConnection: (select: (state: unknown) => unknown) => select({ offline: mock.offline }) }));
vi.mock("../rows/SettingsScopeContext", () => ({ useSettingsScope: () => null }));

const catalog = (name: string) => ({ providers: [{ id: "extra", name, group: "cant_start", labels: [], registered: false, openable: false }] });
let reads: number;
beforeEach(() => {
  reads = 0; mock.offline = false; mock.refresh.mockReset().mockResolvedValue(undefined); mock.check.mockReset().mockResolvedValue(undefined);
  vi.mocked(request).mockReset().mockImplementation(async (method, path) => {
    if (method === "GET" && path === "/api/providers") return catalog(++reads === 1 ? "Old catalogue" : "New catalogue") as never;
    return { catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never;
  });
});

it("updates catalogue rows after repeated checks with unchanged engine status, including offline", async () => {
  mock.offline = true;
  const user = userEvent.setup(); render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  await user.click(within(screen.getByText(/You’re offline/).parentElement!).getByRole("button", { name: "Check again" }));
  expect(await screen.findByText("New catalogue")).toBeVisible();
  await user.click(within(screen.getByRole("status")).getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(reads).toBe(3));
});

it("keeps the prior catalogue and shows the existing error when a check fails", async () => {
  mock.check.mockRejectedValueOnce(new ApiFailure("failed", "Check failed", 500));
  const user = userEvent.setup(); render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Check again" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Check failed");
  expect(screen.getByText("Old catalogue")).toBeVisible();
});

it("retains the catalogue when its refresh fails", async () => {
  const user = userEvent.setup(); render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  vi.mocked(request).mockImplementation(async (method, path) => {
    if (method === "GET" && path === "/api/providers") throw new ApiFailure("failed", "Catalogue unavailable", 500);
    return { catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never;
  });
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Check again" })).toBeEnabled());
  expect(screen.getByText("Old catalogue")).toBeVisible();
});

it("ignores a catalogue reply after unmount and a newer page read", async () => {
  let resolveOld!: (value: never) => void;
  const user = userEvent.setup(); const page = render(<ProvidersSection />);
  expect(await screen.findByText("Old catalogue")).toBeVisible();
  vi.mocked(request).mockImplementation(async (method, path) => {
    if (method === "GET" && path === "/api/providers") return new Promise((resolve) => { resolveOld = resolve; });
    return { catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never;
  });
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(resolveOld).toBeDefined());
  page.unmount();
  vi.mocked(request).mockImplementation(async (method, path) => {
    if (method === "GET" && path === "/api/providers") return catalog("Current catalogue") as never;
    return { catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never;
  });
  render(<ProvidersSection />);
  expect(await screen.findByText("Current catalogue")).toBeVisible();
  await act(async () => { resolveOld(catalog("Stale catalogue") as never); });
  expect(screen.getByText("Current catalogue")).toBeVisible();
  expect(screen.queryByText("Stale catalogue")).toBeNull();
});
