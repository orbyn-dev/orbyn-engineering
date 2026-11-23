import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { CleanupPreview } from "../../../api/types";
import { installMockFetch } from "../../../test-support";
import { GeneralSection } from "./GeneralSection";
import { EnvironmentCleanup, MaintenanceSection } from "../maintenance/Maintenance";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

const requested = (method: string, path: string) =>
  vi.mocked(fetch).mock.calls.some(([url, init]) => (init?.method ?? "GET") === method && String(url).startsWith(path));

it("keeps provider update preferences without a saved-login choice", async () => {
  // A saved-login choice stored by an older version stays stored; the row for it is gone.
  mock.handle("PUT", "/api/preferences", { values: { prefer_saved_login: true } });
  render(<GeneralSection />);
  expect(await screen.findByRole("switch", { name: "Check providers for updates" })).toBeChecked();
  expect(screen.queryByRole("switch", { name: "Prefer saved login" })).toBeNull();
});

it("keeps diagnostics and update controls without requesting a Network inventory", async () => {
  render(<MaintenanceSection />);
  expect(await screen.findByRole("heading", { name: "Diagnostics" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Repair database" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Check for updates" })).toBeEnabled();
  expect(screen.queryByRole("heading", { name: "Network" })).toBeNull();
  expect(requested("GET", "/api/network")).toBe(false);
});

const group = (kind: string, count: number, megabytes: number) => ({ kind, count, bytes: megabytes * 1_000_000, held: 0 });
const previewOf = (groups: ReturnType<typeof group>[], partial = false): CleanupPreview =>
  ({ groups, count: groups.reduce((sum, entry) => sum + entry.count, 0), bytes: 0, partial, entries: [] });
/** The retention the person saved: browser files after 14 days and logs after 7. */
const saveRetention = () => mock.handle("PUT", "/api/preferences", { values: { storage_cleanup: { browser_artifacts_after_days: 14, logs_after_days: 7 } } });

it("asks before deleting old files, names what goes with the service's counts and sizes, and runs the cleanup only on confirm", async () => {
  saveRetention();
  mock.setCleanupPreview(previewOf([group("artifact", 3, 48), group("log", 1, 2), group("worktree", 2, 310)]));
  const user = userEvent.setup();
  render(<EnvironmentCleanup query={{}} disabled={false} />);
  await user.click(await screen.findByRole("button", { name: "Delete now…" }));
  const ask = await screen.findByRole("dialog", { name: "Delete old files now?" });
  expect(requested("POST", "/api/storage/cleanup")).toBe(false);
  expect(within(ask).getByText("Browser artifacts older than 14 days")).toBeVisible();
  expect(within(ask).getByText("3 · 48 MB")).toBeVisible();
  expect(within(ask).getByText("Rotated logs older than 7 days")).toBeVisible();
  expect(within(ask).getByText("1 · 2 MB")).toBeVisible();
  expect(within(ask).getByText("Settled worktrees")).toBeVisible();
  expect(within(ask).getByText("2 · 310 MB")).toBeVisible();
  expect(screen.getByText("Would delete 3 browser artifacts (48 MB), 1 log (2 MB) and 2 settled worktrees (310 MB)")).toBeVisible();
  await user.click(within(ask).getByRole("button", { name: "Delete now" }));
  await waitFor(() => expect(requested("POST", "/api/storage/cleanup")).toBe(true));
});

it("leaves Settled worktrees out when the dry run finds none, and shows a worktree it could not measure in full as at least its size, or the count alone", async () => {
  const user = userEvent.setup();
  saveRetention();
  mock.setCleanupPreview(previewOf([group("artifact", 3, 48), group("log", 1, 2), group("worktree", 0, 0)]));
  const view = render(<EnvironmentCleanup query={{}} disabled={false} />);
  await user.click(await screen.findByRole("button", { name: "Delete now…" }));
  const ask = await screen.findByRole("dialog", { name: "Delete old files now?" });
  expect(within(ask).queryByText("Settled worktrees")).toBeNull();
  expect(screen.getByText("Would delete 3 browser artifacts (48 MB) and 1 log (2 MB)")).toBeVisible();

  view.unmount();
  mock.setCleanupPreview(previewOf([group("worktree", 2, 310)], true));
  const partial = render(<EnvironmentCleanup query={{}} disabled={false} />);
  await user.click(await screen.findByRole("button", { name: "Delete now…" }));
  const second = await screen.findByRole("dialog", { name: "Delete old files now?" });
  expect(within(second).getByText("Settled worktrees")).toBeVisible();
  expect(within(second).getByText("2 · at least 310 MB")).toBeVisible();

  partial.unmount();
  mock.setCleanupPreview(previewOf([group("worktree", 2, 0)], true));
  render(<EnvironmentCleanup query={{}} disabled={false} />);
  await user.click(await screen.findByRole("button", { name: "Delete now…" }));
  const third = await screen.findByRole("dialog", { name: "Delete old files now?" });
  expect(within(third).getByText("2")).toBeVisible();
});
