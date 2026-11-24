import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { AppUpdateStatus } from "../../../generated/AppUpdateStatus";
import { useConnection } from "../../../product/state/connection";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wire, wireWrites } from "../../../wire.test-support";
import { MaintenanceSection } from "./Maintenance";

const hour = (hours: number) => new Date(Date.now() - hours * 3_600_000).toISOString();
const internal = "Orbyn hit an internal error. Check the service log.";
const proc = (pid: number, name: string, running_seconds: number) => ({ pid, parent: 1, name, memory_bytes: 1, running_seconds });

let mock: MockApi;

/** What the service says of the app update; its byte counts are plain JSON numbers, which the generated type names bigint. */
type AppUpdate = Omit<Partial<AppUpdateStatus>, "downloaded" | "total" | "failed_phase"> & { downloaded?: number; total?: number; failed_phase?: AppUpdateStatus["failed_phase"] | null };

/** The app update the service reports. The desktop app is the only one that applies updates itself. */
const setApp = async (extra: AppUpdate = {}, status: { checked_at?: string } = {}) => {
  const app = { current: "0.42.1", latest: null, notes: null, mode: "desktop", can_apply: false, blocked_reason: null, unconfigured: false, ...extra };
  mock.patchUpdates({ ...status, app: app as unknown as AppUpdateStatus });
  await useWorkspace.getState().refreshUpdates();
};
const downloads = () => wireWrites().filter((call) => call.path === "/api/updates/app/download");

beforeEach(async () => {
  mock = installMockFetch();
  // No thread is running unless a test starts from the full set.
  mock.reset("empty");
  useConnection.setState({ offline: false });
  await setApp();
});

it("shows the service facts and lists recent problems from the last 24 hours only when asked", async () => {
  mock.patchDiagnostics({ events: [{ at: hour(1), level: "WARN", target: "t", message: "Codex health check timed out." },
    { at: hour(30), level: "ERROR", target: "t", message: "Too old to list." }] });
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
  mock.patchDiagnostics({ events: [] });
  render(<MaintenanceSection />);
  expect(await screen.findByText("Nothing went wrong in the last 24 hours.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Show" })).toBeNull();
});

it("says so when the service can't be asked", async () => {
  mock.fail("/api/diagnostics", 1, 500, "internal", internal);
  render(<MaintenanceSection />);
  expect(await screen.findByText("Can’t reach the Orbyn service, so there is nothing to show.")).toBeVisible();
});

it("offers Restart to update once the update is downloaded", async () => {
  await setApp({ latest: "0.43.0", can_apply: true, phase: "ready", release_id: "release-1", operation: 1, downloaded: 10, total: 10 });
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  expect(await screen.findByText("0.42.1 · 0.43.0 is downloaded")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Restart to update" }));
  await waitFor(() => expect(wireWrites()).toContainEqual({ method: "POST", path: "/api/updates/app/apply", body: { release_id: "release-1", operation: 1 } }));
});

it("says a failed check with Retry, and offline without offering the check", async () => {
  mock.fail("/api/updates/check", 1, 500, "internal", internal);
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  await user.click(await screen.findByRole("button", { name: "Check for updates" }));
  const banner = (await screen.findByText(new RegExp(`Couldn’t check for updates\\. ${internal}`))).closest<HTMLElement>("[role=alert]")!;
  expect(within(banner).getByRole("button", { name: "Retry" })).toBeVisible();
  useConnection.setState({ offline: true });
  expect(await screen.findByText("You’re offline. Updates are checked when you’re back.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Check for updates" })).toBeDisabled();
});

it("shows the download with its percentage and no button, then Restart to update", async () => {
  await setApp({ latest: "0.43.0", can_apply: false, phase: "downloading", downloaded: 42, total: 100 });
  render(<MaintenanceSection />);
  expect(await screen.findByText(/Downloading 0\.43\.0…/)).toBeVisible();
  expect(screen.getByRole("progressbar", { name: "Download progress" })).toHaveAttribute("aria-valuenow", "42");
  expect(screen.getByText("42%")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Check for updates" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Restart to update" })).toBeNull();
});

it("asks before repairing the database, with Cancel focused", async () => {
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  await user.click(await screen.findByRole("button", { name: "Repair database" }));
  const ask = await screen.findByRole("dialog", { name: "Repair the database?" });
  expect(within(ask).getByText("Orbyn will check and repair its local database. Threads aren’t deleted.")).toBeVisible();
  await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(wireWrites()).toEqual([]);
  await user.click(within(ask).getByRole("button", { name: "Repair database" }));
  await waitFor(() => expect(wireWrites()).toEqual([{ method: "POST", path: "/api/diagnostics/repair" }]));
});

it("lists running processes with a red Stop that asks first, and says when there are none", async () => {
  mock.patchDiagnostics({ processes: [proc(4120, "orbynd", 9000), proc(7, "claude", 3700)] });
  const user = userEvent.setup();
  const { unmount } = render(<MaintenanceSection />);
  expect(await screen.findByText("Running 1h")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop orbynd" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Stop claude" }));
  const ask = await screen.findByRole("dialog", { name: "Stop claude?" });
  expect(within(ask).getByText("It stops now. Threads that use it end their current turn.")).toBeVisible();
  await waitFor(() => expect(within(ask).getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(wireWrites()).toEqual([]);
  await user.click(within(ask).getByRole("button", { name: "Stop" }));
  await waitFor(() => expect(wireWrites()).toEqual([{ method: "POST", path: "/api/diagnostics/processes/7/stop" }]));
  unmount();
  mock.patchDiagnostics({ processes: [proc(4120, "orbynd", 9000)] });
  render(<MaintenanceSection />);
  expect(await screen.findByText("Nothing is running long.")).toBeVisible();
});

it("lists only processes that have run over 2 s, and says nothing is running long otherwise", async () => {
  mock.patchDiagnostics({ processes: [proc(4120, "orbynd", 9000), proc(7, "claude", 5), proc(8, "git", 1), proc(9, "sh", 2)] });
  render(<MaintenanceSection />);
  expect(await screen.findByRole("button", { name: "Stop claude" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop git" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Stop sh" })).toBeNull();
  expect(screen.queryByText("Nothing is running long.")).toBeNull();
});

it("shows Stable as the only live update track, with Nightly dimmed and Planned", async () => {
  await setApp({}, { checked_at: hour(0) });
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
  await setApp({ latest: "0.43.0", release_id: "release-1", operation: 0, phase: "available", can_apply: true });
  render(<MaintenanceSection />);
  await userEvent.click(await screen.findByRole("button", { name: "Download update" }));
  await waitFor(() => expect(downloads()).toEqual([{ method: "POST", path: "/api/updates/app/download", body: { release_id: "release-1", operation: 0 } }]));
  // The service only asked for consent: nothing was installed.
  expect(wireWrites().some((call) => call.path === "/api/updates/app/apply")).toBe(false);
});

it("waits for verified readiness even at 100 percent or with an unknown download total", async () => {
  await setApp({ latest: "0.43.0", phase: "preparing", downloaded: 100, total: 100, can_apply: true });
  render(<MaintenanceSection />);
  expect(await screen.findByText("Preparing 0.43.0…")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Restart to update" })).toBeNull();
});


it("shows unknown-size transfer without a percentage and only the ready phase unlocks offline installation", async () => {
  await setApp({ latest: "0.43.0", phase: "downloading", downloaded: 12, can_apply: false });
  render(<MaintenanceSection />);
  expect(await screen.findByText("Downloading 0.43.0…")).toBeVisible();
  expect(screen.queryByRole("progressbar", { name: "Download progress" })).toBeNull();
  await act(async () => {
    useConnection.setState({ offline: true });
    await setApp({ latest: "0.43.0", phase: "ready", release_id: "r1", operation: 1, can_apply: true });
  });
  expect(await screen.findByRole("button", { name: "Restart to update" })).toBeEnabled();
});

it("serializes download clicks and leaving settings does not cancel the accepted request", async () => {
  await setApp({ latest: "0.43.0", phase: "available", release_id: "release-1", operation: 0, can_apply: true });
  const slow = holdFetch("POST", "/api/updates/app/download");
  const view = render(<MaintenanceSection />);
  await userEvent.dblClick(await screen.findByRole("button", { name: "Download update" }));
  expect(downloads()).toHaveLength(1);
  view.unmount();
  slow.release();
  await waitFor(() => expect(mock.updates.app.phase).toBe("ready"));
  expect(downloads()).toHaveLength(1);
  expect(wire().filter((call) => call.path === "/api/updates/app/download")).toHaveLength(1);
});

it("keeps ready state disabled during active runs and distinguishes install failure from a check error", async () => {
  // The full set of threads has runs going, so the desktop app says to wait for them.
  mock.reset();
  await setApp({ latest: "0.43.0", phase: "ready", release_id: "r1", operation: 1, can_apply: false,
    update_error: "installer refused", failed_phase: "installing" });
  render(<MaintenanceSection />);
  expect(await screen.findByRole("button", { name: "Restart to update" })).toBeDisabled();
  expect(screen.getByText("Wait for active runs to finish before updating.")).toBeVisible();
  expect(screen.getByText("Couldn’t install 0.43.0. Try again.")).toBeVisible();
  expect(screen.queryByText(/Couldn’t check for updates/)).toBeNull();
});

it("uses renewed consent after a changed release and keeps the failed release in failure copy", async () => {
  await setApp({ latest: "0.44.0", phase: "available", release_id: "r2", operation: 1, can_apply: true,
    update_error: "The available update changed. Review the new version before downloading.", failed_phase: null });
  render(<MaintenanceSection />);
  expect(await screen.findByRole("button", { name: "Download update" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Retry download" })).toBeNull();
  await act(async () => {
    await setApp({ latest: "0.44.0", phase: "available", release_id: "r2", operation: 1, can_apply: true,
      update_error: "network", failed_phase: "downloading", failed_version: "0.43.0" });
  });
  expect(await screen.findByText("Couldn’t download 0.43.0. Try again.")).toBeVisible();
  expect(screen.queryByText("Couldn’t download 0.44.0. Try again.")).toBeNull();
});

it("names r1 when its download fails after a newer r2 check", async () => {
  await setApp({ latest: "0.44.0", phase: "available", release_id: "r2", operation: 1, can_apply: true,
    update_error: "network", failed_phase: "downloading", failed_version: "0.43.0" });
  render(<MaintenanceSection />);
  expect(await screen.findByText("Couldn’t download 0.43.0. Try again.")).toBeVisible();
  expect(screen.queryByText("Couldn’t download 0.44.0. Try again.")).toBeNull();
});
