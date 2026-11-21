import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { type HostRow, setEngineHostApi, processLine, runningFor } from "../../enginehost";
import { HostProcessRows, useHostProcesses } from "./HostProcesses";

afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); });

function Rows() {
  const { rows, reload } = useHostProcesses();
  return <ul aria-label="Processes"><HostProcessRows rows={rows} reload={reload} /></ul>;
}

const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
const hosts = (): HostRow[] => [
  { id: "claude", name: "Claude Code", state: "running", since: ago(184), host_version: "0.1.0", protocol: 1, running_threads: 2, can_retry: false },
  { id: "cursor", name: "Cursor", state: "running", pooled: true, since: ago(72), can_retry: false },
  { id: "codex", name: "Codex", state: "restarting", restart: { attempt: 3, limit: 5, next_try_seconds: 4 }, can_retry: false },
  { id: "opencode", name: "OpenCode", state: "cant_start", stops_in_window: 5, can_retry: true },
  { id: "idle", name: "Muse", state: "stopped", can_retry: false },
  { id: "never", name: "Pi", state: "not_started", can_retry: false },
  { id: "old", name: "Old", state: "version_mismatch", can_retry: false },
];

it("lists nothing, and sends no request, while the service reports no host", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  render(<Rows />);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryAllByRole("listitem")).toHaveLength(0);
  expect(fetching).not.toHaveBeenCalled();
});

it("lists a host that is running, restarting or stuck, and not one that is idle, never started or refused", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: hosts() }));
  render(<Rows />);
  expect(await screen.findAllByRole("listitem")).toHaveLength(4);
  expect(screen.getByText("Claude Code engine host")).toBeVisible();
  expect(screen.getByText("Running 3 h 4 min · orbyn-engine-host 0.1.0")).toBeVisible();
  expect(screen.getByText("Running 1 h 12 min · shared by every Cursor thread")).toBeVisible();
  expect(screen.getByText("Restarting · try 3 of 5 · next try in 4 s")).toBeVisible();
  expect(screen.getByText("Can’t start · stopped 5 times in 5 minutes")).toBeVisible();
  expect(screen.queryByText("Muse engine host")).toBeNull();
});

it("offers Restart and Stop on a running host, only Stop while restarting, only Restart when it can’t start", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: hosts() }));
  render(<Rows />);
  await screen.findAllByRole("listitem");
  const of = (name: string) => within(screen.getByText(name).closest("li")!);
  expect(of("Claude Code engine host").getByRole("button", { name: "Restart Claude Code engine host" })).toBeVisible();
  expect(of("Claude Code engine host").getByRole("button", { name: "Stop Claude Code engine host" })).toBeVisible();
  expect(of("Codex engine host").queryByRole("button", { name: /^Restart/ })).toBeNull();
  expect(of("Codex engine host").getByRole("button", { name: /^Stop/ })).toBeVisible();
  expect(of("OpenCode engine host").getByRole("button", { name: /^Restart/ })).toBeVisible();
  expect(of("OpenCode engine host").queryByRole("button", { name: /^Stop/ })).toBeNull();
});

it("Restart is one write per click", async () => {
  const mock = createEngineHostMock({ hosts: hosts(), delayMs: 20 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<Rows />);
  await user.dblClick(await screen.findByRole("button", { name: "Restart OpenCode engine host" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:opencode"]));
});

it("Stop asks first, says what it costs with Cancel focused, then writes once", async () => {
  const mock = createEngineHostMock({ hosts: hosts(), delayMs: 20 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<Rows />);
  await user.click(await screen.findByRole("button", { name: "Stop Claude Code engine host" }));
  const dialog = await screen.findByRole("dialog", { name: "Stop the Claude Code engine host?" });
  expect(within(dialog).getByText(/^It stops now\. The 2 threads running on it end with “The engine host stopped\. Your work in the project is kept; send your message again\.” It starts again with the next message\.$/)).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  expect(mock.writes).toEqual([]);
  await user.dblClick(within(dialog).getByRole("button", { name: "Stop" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["stop:claude"]);
});

it("reads a host name the service chose as cut text, never markup", async () => {
  const hostile = `<img src=x onerror=alert(1)>${"y".repeat(200)}`;
  setEngineHostApi(createEngineHostMock({ hosts: [{ id: "x", name: hostile, state: "cant_start", can_retry: true }] }));
  const { baseElement } = render(<Rows />);
  await screen.findAllByRole("listitem");
  expect(baseElement.querySelector("img")).toBeNull();
});

it("words the running time as whole hours and minutes", () => {
  const now = Date.parse("2026-11-21T12:00:00Z");
  expect(runningFor("2026-11-21T11:59:15Z", now)).toBe("45 s");
  expect(runningFor("2026-11-21T11:22:00Z", now)).toBe("38 min");
  expect(runningFor("2026-11-21T08:56:00Z", now)).toBe("3 h 4 min");
  expect(runningFor("not a time", now)).toBe("");
  expect(processLine({ id: "a", name: "A", state: "running", since: "2026-11-21T11:00:00Z", can_retry: false }, now)).toBe("Running 1 h 0 min · orbyn-engine-host");
});

it("restarting a running host with threads asks first; one with none restarts at once", async () => {
  const mock = createEngineHostMock({ hosts: hosts() });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<Rows />);
  await user.click(await screen.findByRole("button", { name: "Restart Cursor engine host" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:cursor"]));
  await user.click(await screen.findByRole("button", { name: "Restart Claude Code engine host" }));
  const dialog = await screen.findByRole("dialog", { name: "Restart the Claude Code engine host?" });
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  expect(mock.writes).toEqual(["retry:cursor"]);
  await user.click(within(dialog).getByRole("button", { name: "Restart" }));
  await waitFor(() => expect(mock.writes).toEqual(["retry:cursor", "retry:claude"]));
});

it("cuts a failure message the service chose before the Stop question shows it", async () => {
  const mock = createEngineHostMock({ hosts: hosts() });
  mock.stop = async () => { throw new Error(`<b>x</b>${"w".repeat(900)}`); };
  setEngineHostApi(mock);
  const user = userEvent.setup();
  const { baseElement } = render(<Rows />);
  await user.click(await screen.findByRole("button", { name: "Stop Cursor engine host" }));
  await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Stop" }));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent!.length).toBeLessThan(330);
  expect(baseElement.querySelector("dialog b")).toBeNull();
});
