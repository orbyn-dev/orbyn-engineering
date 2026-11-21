import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, ApiFailure, request } from "../../../api/client";
import { useConnection } from "../../../product/state/connection";
import { useWorkspace } from "../../../product/state/workspace";
import { MaintenanceSection } from "./Maintenance";
import { scopedFixture } from "../rows/scopedPreferenceFixture.test-support";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const hour = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();
const diagnostics = (events: unknown[]) => ({ version: "0.42.1", pid: 1, active_runs: 2, terminals: 0,
  database: { bytes: 41_000_000, reclaimable_bytes: 0 }, history: [{ at: hour(0), memory_bytes: 182_000_000, processes: 1 }],
  processes: [], events });
const app = (extra: object = {}) => ({ library: { incoming: [], blocked_reason: null }, engines: [], checked_at: null, error: null,
  app: { current: "0.42.1", latest: null, notes: null, mode: "browser", can_apply: false, blocked_reason: null, unconfigured: false, ...extra } }) as never;

const serve = (read: () => unknown) => vi.mocked(request).mockImplementation(async (_method, path) => {
  if (path === "/api/diagnostics") return read() as never;
  if (path === "/api/settings/preferences/query") return scopedFixture([]) as never;
  return null as never;
});

beforeEach(() => {
  vi.mocked(request).mockReset();
  vi.spyOn(api, "settings").mockResolvedValue({ background_update_checks: true } as Awaited<ReturnType<typeof api.settings>>);
  useConnection.setState({ offline: false });
  useWorkspace.getState().putUpdates(app());
});
afterEach(() => vi.restoreAllMocks());

it("shows the service facts and lists recent problems from the last 24 hours only when asked", async () => {
  serve(() => diagnostics([{ at: hour(1), level: "WARN", target: "t", message: "Codex health check timed out." },
    { at: hour(30), level: "ERROR", target: "t", message: "Too old to list." }]));
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  const card = await screen.findByRole("group", { name: "Service" });
  expect(within(card).getByText("Running · 0.42.1")).toBeVisible();
  expect(within(card).getByText("2 threads running")).toBeVisible();
  expect(within(card).getByText("41 MB")).toBeVisible();
  expect(within(card).getByText("182 MB")).toBeVisible();
  expect(screen.getByText("1 in the last 24 hours")).toBeVisible();
  expect(screen.queryByText("Codex health check timed out.")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Show" }));
  expect(screen.getByText("Codex health check timed out.")).toBeVisible();
  expect(screen.queryByText("Too old to list.")).toBeNull();
});

it("says there are no problems instead of showing an empty list", async () => {
  serve(() => diagnostics([]));
  render(<MaintenanceSection />);
  expect(await screen.findByText("Nothing went wrong in the last 24 hours.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Show" })).toBeNull();
});

it("says so when the service can't be asked", async () => {
  vi.mocked(request).mockImplementation(async (_method, path) => {
    if (path === "/api/diagnostics") throw new ApiFailure("network_error", "down", 0);
    return scopedFixture([]) as never;
  });
  render(<MaintenanceSection />);
  expect(await screen.findByText("Can’t reach the Orbyn service, so there is nothing to show.")).toBeVisible();
});

it("offers Restart to update once the update is downloaded", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", can_apply: true, phase: "ready", release_id: "release-1", operation: 1, downloaded: 10, total: 10 }));
  const apply = vi.spyOn(api, "applyAppUpdate").mockResolvedValue(undefined);
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  expect(await screen.findByText("0.42.1 · 0.43.0 is downloaded")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Restart to update" }));
  await waitFor(() => expect(apply).toHaveBeenCalled());
});

it("says a failed check with Retry, and offline without offering the check", async () => {
  serve(() => diagnostics([]));
  vi.spyOn(api, "checkUpdates").mockRejectedValue(new ApiFailure("bad_gateway", "The update server didn’t answer.", 502));
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  await user.click(await screen.findByRole("button", { name: "Check for updates" }));
  const banner = (await screen.findByText(/Couldn’t check for updates\. The update server didn’t answer\./)).closest<HTMLElement>("[role=alert]")!;
  expect(within(banner).getByRole("button", { name: "Retry" })).toBeVisible();
  useConnection.setState({ offline: true });
  expect(await screen.findByText("You’re offline. Updates are checked when you’re back.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
});

it("shows the download with its percentage and no button, then Restart to update", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", can_apply: false, phase: "downloading", downloaded: 42, total: 100 }));
  render(<MaintenanceSection />);
  expect(await screen.findByText(/Downloading 0\.43\.0…/)).toBeVisible();
  expect(screen.getByRole("progressbar", { name: "Download progress" })).toHaveAttribute("aria-valuenow", "42");
  expect(screen.getByText("42%")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Restart to update" })).toBeNull();
});

it("asks before repairing the database, with Cancel focused", async () => {
  serve(() => diagnostics([]));
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  await user.click(await screen.findByRole("button", { name: "Repair database" }));
  const ask = await screen.findByRole("dialog", { name: "Repair the database?" });
  expect(within(ask).getByText("Orbyn will check and repair its local database. Threads aren’t deleted.")).toBeVisible();
  await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(vi.mocked(request)).not.toHaveBeenCalledWith("POST", "/api/diagnostics/repair");
  await user.click(within(ask).getByRole("button", { name: "Repair database" }));
  await waitFor(() => expect(vi.mocked(request)).toHaveBeenCalledWith("POST", "/api/diagnostics/repair"));
});

it("lists running processes with a red Stop that asks first, and says when there are none", async () => {
  const proc = (pid: number, name: string, running_seconds: number) => ({ pid, parent: 1, name, memory_bytes: 1, running_seconds });
  serve(() => ({ ...diagnostics([]), processes: [proc(1, "orbynd", 9000), proc(7, "claude", 3700)] }));
  const user = userEvent.setup();
  const { unmount } = render(<MaintenanceSection />);
  expect(await screen.findByText("Running 1h")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop orbynd" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Stop claude" }));
  const ask = await screen.findByRole("dialog", { name: "Stop claude?" });
  expect(within(ask).getByText("It stops now. Threads that use it end their current turn.")).toBeVisible();
  await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(vi.mocked(request)).not.toHaveBeenCalledWith("POST", "/api/diagnostics/processes/7/stop");
  await user.click(within(ask).getByRole("button", { name: "Stop" }));
  await waitFor(() => expect(vi.mocked(request)).toHaveBeenCalledWith("POST", "/api/diagnostics/processes/7/stop"));
  unmount();
  serve(() => ({ ...diagnostics([]), processes: [proc(1, "orbynd", 9000)] }));
  render(<MaintenanceSection />);
  expect(await screen.findByText("Nothing is running long.")).toBeVisible();
});

it("lists only processes that have run over 2 s, and says nothing is running long otherwise", async () => {
  const proc = (pid: number, name: string, running_seconds: number) => ({ pid, parent: 1, name, memory_bytes: 1, running_seconds });
  serve(() => ({ ...diagnostics([]), processes: [proc(1, "orbynd", 9000), proc(7, "claude", 5), proc(8, "git", 1), proc(9, "sh", 2)] }));
  render(<MaintenanceSection />);
  expect(await screen.findByRole("button", { name: "Stop claude" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop git" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Stop sh" })).toBeNull();
  expect(screen.queryByText("Nothing is running long.")).toBeNull();
});

it("shows Stable as the only live update track, with Nightly dimmed and Planned", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates({ ...(app() as object), checked_at: hour(0) } as never);
  render(<MaintenanceSection />);
  const tracks = await screen.findByRole("radiogroup", { name: "Update track" });
  expect(within(tracks).getByRole("radio", { name: /Stable/ })).toBeChecked();
  expect(within(tracks).getByText("Tested releases. Recommended.")).toBeVisible();
  const nightly = within(tracks).getByRole("radio", { name: /Nightly/ });
  expect(nightly).not.toBeChecked();
  expect(nightly).toHaveAttribute("aria-disabled", "true");
  expect(within(nightly).getByText("Planned")).toBeVisible();
  expect(screen.getByText("0.42.1 · up to date")).toBeVisible();
});


it("offers explicit download consent and never treats received bytes as verified readiness", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", release_id: "release-1", operation: 0, phase: "available", can_apply: true }));
  const download = vi.spyOn(api, "downloadAppUpdate").mockResolvedValue(undefined);
  render(<MaintenanceSection />);
  await userEvent.click(await screen.findByRole("button", { name: "Download update" }));
  expect(download).toHaveBeenCalledWith({ release_id: "release-1", operation: 0 });
});

it("waits for verified readiness even at 100 percent or with an unknown download total", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", phase: "preparing", downloaded: 100, total: 100, can_apply: true }));
  render(<MaintenanceSection />);
  expect(await screen.findByText("Preparing 0.43.0…")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Restart to update" })).toBeNull();
});


it("shows unknown-size transfer without a percentage and only the ready phase unlocks offline installation", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", phase: "downloading", downloaded: 12, can_apply: false }));
  render(<MaintenanceSection />);
  expect(await screen.findByText("Downloading 0.43.0…")).toBeVisible();
  expect(screen.queryByRole("progressbar", { name: "Download progress" })).toBeNull();
  act(() => {
    useConnection.setState({ offline: true });
    useWorkspace.getState().putUpdates(app({ latest: "0.43.0", phase: "ready", release_id: "r1", operation: 1, can_apply: true }));
  });
  expect(await screen.findByRole("button", { name: "Restart to update" })).toBeEnabled();
});

it("serializes download clicks and leaving settings does not cancel the accepted request", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", phase: "available", release_id: "r1", operation: 0, can_apply: true }));
  let finish = () => {};
  const download = vi.spyOn(api, "downloadAppUpdate").mockImplementation(() => new Promise<void>((resolve) => { finish = resolve; }));
  const view = render(<MaintenanceSection />);
  await userEvent.dblClick(await screen.findByRole("button", { name: "Download update" }));
  expect(download).toHaveBeenCalledTimes(1);
  view.unmount();
  await act(async () => { finish(); });
  expect(download).toHaveBeenCalledTimes(1);
});

it("keeps ready state disabled during active runs and distinguishes install failure from a check error", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.43.0", phase: "ready", release_id: "r1", operation: 1, can_apply: false,
    blocked_reason: "Finish or stop active runs first.", update_error: "installer refused", failed_phase: "installing" }));
  render(<MaintenanceSection />);
  expect(await screen.findByRole("button", { name: "Restart to update" })).toBeDisabled();
  expect(screen.getByText("Wait for active runs to finish before updating.")).toBeVisible();
  expect(screen.getByText("Couldn’t install 0.43.0. Try again.")).toBeVisible();
  expect(screen.queryByText(/Couldn’t check for updates/)).toBeNull();
});

it("uses renewed consent after a changed release and keeps the failed release in failure copy", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.44.0", phase: "available", release_id: "r2", operation: 1, can_apply: true,
    update_error: "The available update changed. Review the new version before downloading.", failed_phase: null }));
  render(<MaintenanceSection />);
  expect(await screen.findByRole("button", { name: "Download update" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Retry download" })).toBeNull();
  act(() => { useWorkspace.getState().putUpdates(app({ latest: "0.44.0", phase: "available", release_id: "r2", operation: 1, can_apply: true,
    update_error: "network", failed_phase: "downloading", failed_version: "0.43.0" })); });
  expect(await screen.findByText("Couldn’t download 0.43.0. Try again.")).toBeVisible();
  expect(screen.queryByText("Couldn’t download 0.44.0. Try again.")).toBeNull();
});

it("names r1 when its download fails after a newer r2 check", async () => {
  serve(() => diagnostics([]));
  useWorkspace.getState().putUpdates(app({ latest: "0.44.0", phase: "available", release_id: "r2", operation: 1, can_apply: true,
    update_error: "network", failed_phase: "downloading", failed_version: "0.43.0" }));
  render(<MaintenanceSection />);
  expect(await screen.findByText("Couldn’t download 0.43.0. Try again.")).toBeVisible();
  expect(screen.queryByText("Couldn’t download 0.44.0. Try again.")).toBeNull();
});
