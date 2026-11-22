import { act, render, screen, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { EngineHostState } from "../../generated/EngineHostState";
import type { EngineHostStatus } from "../../generated/EngineHostStatus";
import { EngineHostCard } from "../settings/engineHost/HostCard";
import { EngineHostGroup } from "../settings/engineHost/HostGroup";
import { setEngineHostApi, useEngineHosts } from "./index";
import { hostCrashes, hostRows, toHostRow } from "./hostStatus";
import { processLine, stateLine } from "./hostText";

const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
const status = (patch: Partial<EngineHostStatus> & { family: string; state: EngineHostState }): EngineHostStatus => ({ restart_count: 0, ...patch });
const answer = (rows: unknown) => vi.fn(async (_url: string, _init?: RequestInit) => json(rows));

beforeEach(() => { setEngineHostApi(null); });
afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); vi.useRealTimers(); });

const STATES: EngineHostState[] = ["not_started", "starting", "running", "stopped", "crashed", "restarting", "cant_start", "version_mismatch", "not_used"];
const CHIP: Record<EngineHostState, string> = { not_started: "Not started", starting: "Starting", running: "Running", stopped: "Stopped", crashed: "Crashed",
  restarting: "Restarting", cant_start: "Can’t start", version_mismatch: "Version mismatch", not_used: "Not used" };

it("reads GET /api/engine-host and draws each of the nine states with its chip", async () => {
  const fetching = answer(STATES.map((state) => status({ family: `f-${state}`, state })));
  vi.stubGlobal("fetch", fetching);
  render(<EngineHostGroup />);
  const table = await screen.findByRole("table", { name: "Engine host" });
  for (const state of STATES) {
    const row = within(table).getByText(`f-${state}`, { selector: ".name" }).closest("[role=row]") as HTMLElement;
    expect(within(row).getByText(CHIP[state])).toBeVisible();
  }
  expect(fetching.mock.calls[0]![0]).toBe("/api/engine-host");
  expect(fetching.mock.calls[0]![1]!.method).toBe("GET");
});

it("maps the service names onto the row and writes nothing the service did not send", () => {
  const crash = { at: "2026-11-21T09:41:00Z", error_kind: "closed_output", message: "The engine host closed its output", stderr_tail: "boom", exit_code: 3 } as const;
  const row = toHostRow(status({ family: "codex", state: "crashed", host_version: "0.1.0", restart_count: 2, last_crash: crash, protocol_app: 1, protocol_host: 2, detail: "d", action: "Update Orbyn" }))!;
  expect(row).toMatchObject({ id: "codex", name: "Codex", state: "crashed", host_version: "0.1.0", restart_count: 2, protocol: 2, app_protocol: 1, detail: "d", action: "Update Orbyn",
    stopped_at: crash.at, can_retry: false });
  expect(row.last_error).toEqual({ message: crash.message, causes: [] });
  expect(row.since).toBeUndefined();
  expect(row.sdk).toBeUndefined();
  expect(row.capabilities).toBeUndefined();
  expect(row.running_threads).toBeUndefined();
  expect(toHostRow({ state: "running" } as never)).toBeNull();
  expect(toHostRow(null as never)).toBeNull();
});

it("says what each slice 1 state says, from the fields the service returns", async () => {
  vi.stubGlobal("fetch", answer([
    status({ family: "claude", state: "running", host_version: "0.1.0" }),
    status({ family: "codex", state: "not_used", not_used: "unsupported_platform" }),
    status({ family: "pi", state: "version_mismatch", protocol_app: 1, protocol_host: 2, action: "Update Orbyn" }),
    status({ family: "acp", state: "cant_start", detail: "It would not start.", last_crash: { at: "2026-11-21T09:41:00Z", error_kind: "hello_timeout", message: "No hello", stderr_tail: "" } }),
  ]));
  render(<><EngineHostGroup /><EngineHostCard engineId="claude" /><EngineHostCard engineId="codex" /><EngineHostCard engineId="pi" /><EngineHostCard engineId="acp" /></>);
  const table = await screen.findByRole("table", { name: "Engine host" });
  // No `since` and no protocol number: the line says only what is known, with no dangling words.
  expect(within(table).getByText("Host 0.1.0")).toBeVisible();
  expect(within(table).getByText("The engine host does not match this app version. Host protocol 2, app protocol 1.")).toBeVisible();
  expect(within(table).getByText("It would not start. Last error: No hello.")).toBeVisible();
  const cards = screen.getAllByRole("group", { name: "Engine host" });
  expect(within(cards[1]!).getByText("This build has no engine host for this platform.")).toBeVisible();
  expect(within(cards[2]!).getByText("Update Orbyn")).toBeVisible();
  expect(within(cards[2]!).getByText("Protocol 1")).toBeVisible();
  expect(screen.queryByText("Planned")).toBeNull();
});

it("draws a state this app does not know as Unknown, and does not crash", async () => {
  vi.stubGlobal("fetch", answer([status({ family: "x", state: "hibernating" as never }), status({ family: "constructor", state: "toString" as never })]));
  render(<EngineHostGroup />);
  const table = await screen.findByRole("table", { name: "Engine host" });
  expect(within(table).getAllByText("Unknown")).toHaveLength(2);
  expect(within(table).getByText("constructor", { selector: ".name" })).toBeVisible();
});

it("answers nothing for a 404, and an error for any other failure", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => json({ code: "not_found", message: "no" }, 404)));
  await expect(hostRows()).resolves.toBeNull();
  await expect(hostCrashes()).resolves.toEqual([]);
  vi.stubGlobal("fetch", vi.fn(async () => json({ code: "boom", message: "no" }, 500)));
  await expect(hostRows()).rejects.toMatchObject({ status: 500 });
  vi.stubGlobal("fetch", answer({ not: "a list" }));
  await expect(hostRows()).resolves.toBeNull();
});

it("shows the redacted stderr tail as text, capped, and never as markup", async () => {
  const hostile = `<img src=x onerror="window.__owned=1"><script>window.__owned=1</script>${"x".repeat(2000)}`;
  vi.stubGlobal("fetch", answer([status({ family: "codex", state: "crashed", restart_count: 1,
    last_crash: { at: "2026-11-21T09:41:00Z", error_kind: "exited", message: "<b>bold</b> stopped", stderr_tail: hostile, signal: 9 } })]));
  const { container } = render(<EngineHostCard engineId="codex" />);
  await screen.findByText("<b>bold</b> stopped");
  act(() => { screen.getByRole("button", { name: "Show details" }).click(); });
  expect(screen.getByText("Stopped by signal 9 · The host exited")).toBeVisible();
  const tail = container.querySelector("pre.eh-stack")!;
  expect(tail.textContent!.startsWith("<img src=x")).toBe(true);
  expect(tail.textContent!.length).toBeLessThanOrEqual(1025);
  expect(container.querySelector("img, script, b")).toBeNull();
  expect((window as unknown as { __owned?: number }).__owned).toBeUndefined();
});

it("lists each family's last crash for Maintenance", async () => {
  vi.stubGlobal("fetch", answer([status({ family: "claude", state: "running" }),
    status({ family: "codex", state: "restarting", last_crash: { at: "2026-11-21T09:41:00Z", error_kind: "stalled", message: "m", stderr_tail: "t", exit_code: 1 } })]));
  expect(await hostCrashes()).toEqual([{ id: "codex:2026-11-21T09:41:00Z", family: "codex", at: "2026-11-21T09:41:00Z", message: "m", error_kind: "stalled", exit_code: 1, signal: undefined, stderr_tail: "t" }]);
});

function Poller() {
  const { hosts } = useEngineHosts();
  return <p>{hosts === null ? "none" : hosts.map((row) => row.state).join(",")}</p>;
}

it("polls every 1.5 s while a host is starting, and stops once it settles", async () => {
  vi.useFakeTimers();
  const order = ["starting", "restarting", "running"];
  let calls = 0;
  const fetching = vi.fn(async () => json([status({ family: "claude", state: (order[Math.min(calls++, order.length - 1)] as EngineHostState) })]));
  vi.stubGlobal("fetch", fetching);
  render(<Poller />);
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(screen.getByText("starting")).toBeVisible();
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  expect(screen.getByText("restarting")).toBeVisible();
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  expect(screen.getByText("running")).toBeVisible();
  const settled = fetching.mock.calls.length;
  await act(async () => { await vi.advanceTimersByTimeAsync(10_000); });
  expect(fetching.mock.calls.length).toBe(settled);
});

it("keeps the newest answer when an older request lands late", async () => {
  vi.useFakeTimers();
  const pending: Array<(response: Response) => void> = [];
  const fetching = vi.fn(() => new Promise<Response>((resolve) => { pending.push(resolve); }));
  vi.stubGlobal("fetch", fetching);
  render(<Poller />);
  await act(async () => { await vi.advanceTimersByTimeAsync(0); });
  expect(pending).toHaveLength(1);
  // Answer the first as "starting" so polling begins; the second poll goes out and is answered before a third, slower one.
  await act(async () => { pending[0]!(json([status({ family: "claude", state: "starting" })])); await vi.advanceTimersByTimeAsync(0); });
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  await act(async () => { await vi.advanceTimersByTimeAsync(1500); });
  expect(pending).toHaveLength(3);
  await act(async () => { pending[2]!(json([status({ family: "claude", state: "running" })])); await vi.advanceTimersByTimeAsync(0); });
  expect(screen.getByText("running")).toBeVisible();
  await act(async () => { pending[1]!(json([status({ family: "claude", state: "starting" })])); await vi.advanceTimersByTimeAsync(0); });
  expect(screen.getByText("running")).toBeVisible();
});

it("an unknown state is only a chip: no line and no action on the list, only the State row on the card (enginehost-unknown-*)", async () => {
  vi.stubGlobal("fetch", answer([status({ family: "codex", state: "hibernating" as never, host_version: "0.1.0", protocol_host: 1, restart_count: 4 })]));
  render(<><EngineHostGroup /><EngineHostCard engineId="codex" /></>);
  const table = await screen.findByRole("table", { name: "Engine host" });
  const row = within(table).getByText("Codex", { selector: ".name" }).closest("[role=row]") as HTMLElement;
  expect(row.querySelector(".eh-sub")?.textContent).toBe("");
  expect(within(row).queryByRole("button")).toBeNull();
  const card = screen.getByRole("group", { name: "Engine host" });
  expect(within(card).getByText("State")).toBeVisible();
  expect(within(card).getByText("Unknown")).toBeVisible();
  for (const left of ["Host", "Engine SDK", "It can", "Last error", "Action"]) expect(within(card).queryByText(left)).toBeNull();
});

it("reads the try, limit and wait the service sends for a restarting host", () => {
  const one = toHostRow(status({ family: "codex", state: "restarting", restart_count: 1 }))!;
  const two = toHostRow(status({ family: "codex", state: "restarting", restart_count: 2 }))!;
  const none = toHostRow(status({ family: "codex", state: "restarting" }))!;
  expect(two.restart).toBeUndefined();
  expect(stateLine(one)).toBe("Waiting to start it again. Restarted 1 time since Orbyn started.");
  expect(stateLine(two)).toBe("Waiting to start it again. Restarted 2 times since Orbyn started.");
  expect(stateLine(none)).toBe("Waiting to start it again.");
  expect(processLine(two, Date.now())).toBe("Restarting · restarted 2 times");
  const sent = toHostRow({ ...status({ family: "codex", state: "restarting", restart_count: 2 }), restart: { attempt: 3, limit: 5, next_try_seconds: 4 } } as never)!;
  expect(sent.restart).toEqual({ attempt: 3, limit: 5, next_try_seconds: 4 });
  expect(stateLine(sent)).toBe("Restart 3 of 5 · next try in 4 s");
  expect(processLine(sent, Date.now())).toBe("Restarting · try 3 of 5 · next try in 4 s");
});

it("maps the host hello and the run fallback the service now sends", () => {
  const local = (hours: number, minutes: number) => new Date(2026, 10, 21, hours, minutes).toISOString();
  const row = toHostRow(status({
    family: "claude", state: "running", since: local(9, 12), pooled: true,
    sdk: { name: "@anthropic-ai/claude-agent-sdk", version: "0.3.321" },
    capabilities: ["steer", "fork"], has_driver: true,
  }))!;
  expect(row).toMatchObject({
    since: local(9, 12), pooled: true, has_driver: true,
    sdk: { name: "@anthropic-ai/claude-agent-sdk", version: "0.3.321" },
    capabilities: ["steer", "fork"],
  });
  expect(stateLine(row, new Date(local(10, 0)))).toBe("Since 09:12 today");
  expect(processLine(row, Date.parse(local(9, 22)))).toBe("Running 10 min · shared by every Claude Code thread");
  expect(toHostRow(status({ family: "cursor", state: "cant_start", sdk_missing: true, stops_in_window: 5 }))).toMatchObject({
    sdk_missing: true, stops_in_window: 5,
  });
});
