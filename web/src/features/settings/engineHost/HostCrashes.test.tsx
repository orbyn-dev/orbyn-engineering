import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it } from "vitest";
import { createEngineHostMock, crashFixtures } from "../../../../mock/engineHost";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { MaintenanceSection } from "../maintenance/Maintenance";
import { setEngineHostApi } from "../../enginehost";

const minutes = (count: number) => new Date(Date.now() - count * 60_000).toISOString();
let mock: MockApi;

beforeEach(async () => {
  mock = installMockFetch();
  // One ordinary problem in the service's log, and only the service itself running.
  mock.patchDiagnostics({
    events: [{ at: minutes(30), level: "WARN", target: "t", message: "Provider health check for Codex timed out after 10 s." }],
    processes: [{ pid: 4120, parent: null, name: "orbynd", memory_bytes: 182_000_000, running_seconds: 5400 }],
  });
  useConnection.setState({ offline: false });
  await useWorkspace.getState().refreshUpdates();
});
afterEach(() => setEngineHostApi(null));

it("leaves Recent problems as it was while the service reports no host crash", async () => {
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  expect(await screen.findByText("1 in the last 24 hours")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Show" }));
  expect(screen.queryByRole("button", { name: "Show details" })).toBeNull();
});

it("adds a row for each host crash, and Show opens how it ended and its last words, with no stack", async () => {
  const crash = { ...crashFixtures()[0]!, at: minutes(5) };
  setEngineHostApi(createEngineHostMock({ crashes: [crash] }));
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  expect(await screen.findByText("2 in the last 24 hours")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Show" }));
  const table = screen.getByRole("table", { name: "Recent problems" });
  const row = within(table).getByText("The Codex engine host stopped: the engine host closed its output.").closest("[role=row]") as HTMLElement;
  expect(within(table).getAllByRole("row")).toHaveLength(2);
  expect(within(table).getByText("Provider health check for Codex timed out after 10 s.")).toBeVisible();
  expect(screen.queryByText(/HostProcess\.onExit/)).toBeNull();
  // Only a host crash has details; the ordinary problem row is unchanged.
  expect(screen.getAllByRole("button", { name: "Show details" })).toHaveLength(1);
  await user.click(within(row).getByRole("button", { name: "Show details" }));
  expect(screen.getByText("Exited with code 3 · The host closed its output")).toBeVisible();
  expect(screen.getByText("Last words from the host")).toBeVisible();
  expect(screen.getByText(/write EPIPE/)).toBeVisible();
  expect(screen.queryByText("Stack")).toBeNull();
  await user.click(within(row).getByRole("button", { name: "Hide details" }));
  expect(screen.queryByText("Last words from the host")).toBeNull();
});

it("copies the report with the open crash, last words included, and without it when closed", async () => {
  const crash = { ...crashFixtures()[0]!, at: minutes(5) };
  setEngineHostApi(createEngineHostMock({ crashes: [crash] }));
  const written: string[] = [];
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async (text: string) => { written.push(text); } } });
  await user.click(await screen.findByRole("button", { name: "Show" }));
  await user.click(screen.getByRole("button", { name: "Copy report" }));
  expect(written[0]).toContain("The Codex engine host stopped: the engine host closed its output.");
  expect(written[0]).not.toContain("write EPIPE");
  await user.click(screen.getByRole("button", { name: "Show details" }));
  await user.click(screen.getByRole("button", { name: "Copy report" }));
  expect(written[1]).toContain("Exited with code 3 · The host closed its output");
  expect(written[1]).toContain("write EPIPE");
});

it("lists the engine hosts under Processes, above the ordinary ones, and leaves Processes alone while none is reported", async () => {
  render(<MaintenanceSection />);
  expect(await screen.findByText("Nothing is running long.")).toBeVisible();
  setEngineHostApi(createEngineHostMock({ hosts: [{ id: "claude", name: "Claude Code", state: "running", since: minutes(184), host_version: "0.1.0", can_retry: false }] }));
  const list = await screen.findByRole("list", { name: "Processes" });
  expect(within(list).getByText("Claude Code engine host")).toBeVisible();
  expect(within(list).getByRole("button", { name: "Stop Claude Code engine host" })).toBeVisible();
});

it("shows how it ended alone for a crash that left no words, a signal as Stopped by signal N, and no Show details for one with neither", async () => {
  const base = { ...crashFixtures()[0]!, at: minutes(5) };
  const killed = { ...base, id: "cursor:killed", family: "cursor", at: minutes(6), message: "The Cursor engine host stopped.", exit_code: undefined, signal: 9, error_kind: "stalled" as const, stderr_tail: "" };
  const bare = { ...base, id: "opencode:bare", family: "opencode", at: minutes(7), message: "The OpenCode engine host stopped.", exit_code: undefined, signal: undefined, error_kind: undefined as never, stderr_tail: "" };
  setEngineHostApi(createEngineHostMock({ crashes: [killed, bare] }));
  const user = userEvent.setup();
  render(<MaintenanceSection />);
  await user.click(await screen.findByRole("button", { name: "Show" }));
  const table = screen.getByRole("table", { name: "Recent problems" });
  // One toggle: the host that carried neither a way it ended nor words has nothing to open.
  expect(screen.getAllByRole("button", { name: "Show details" })).toHaveLength(1);
  const row = within(table).getByText("The Cursor engine host stopped.").closest("[role=row]") as HTMLElement;
  await user.click(within(row).getByRole("button", { name: "Show details" }));
  expect(screen.getByText("Stopped by signal 9 · The host stopped answering")).toBeVisible();
  expect(screen.queryByText("Last words from the host")).toBeNull();
  expect(within(table).getByText("The OpenCode engine host stopped.").closest("[role=row]")!.querySelector("button")).toBeNull();
});
