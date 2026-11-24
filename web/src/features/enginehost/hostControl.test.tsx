import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PERSON_HEADER } from "../../api/personKey";
import type { EngineHostState } from "../../generated/EngineHostState";
import { EngineHostGroup } from "../settings/engineHost/HostGroup";
import { HostProcessRows, useHostProcesses } from "../settings/engineHost/HostProcesses";
import { engineHostApi, setEngineHostApi } from "./index";
import { toHostRow } from "./hostStatus";
import { ComposerHostLine } from "./HostUi";

type Call = { method: string; path: string; headers: Headers; body: string | undefined };
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json" } });
const refusal = (status: number, code: string, message: string) => json({ code, message }, status);

/** A stub service: the engine host list is whatever `hosts` holds, and a write answers from `writes` (or a status). */
function service(hosts: { family: string; state: EngineHostState; restart_count?: number }[], writes: Response[] = []) {
  const calls: Call[] = [];
  const queue = [...writes];
  const fetching = vi.fn(async (url: string, init: RequestInit) => {
    calls.push({ method: init.method ?? "GET", path: url, headers: new Headers(init.headers), body: init.body as string | undefined });
    if ((init.method ?? "GET") === "GET") return json(hosts.map((row) => ({ restart_count: 0, ...row })));
    return queue.shift() ?? json({ family: "claude", state: "stopped", restart_count: 0 });
  });
  vi.stubGlobal("fetch", fetching);
  return { calls, writes: () => calls.filter((call) => call.method !== "GET") };
}

function Processes() { const { rows, reload } = useHostProcesses(); return <ul aria-label="Processes"><HostProcessRows rows={rows} reload={reload} /></ul>; }

beforeEach(() => { setEngineHostApi(null); });
afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); });

it("the real client is writable and sends Stop as one POST with the person key and stop_background false, while the read carries no key", async () => {
  const stub = service([{ family: "claude", state: "running", restart_count: 1 }]);
  expect(engineHostApi().writable).toBe(true);
  await engineHostApi().hosts();
  await engineHostApi().stop("claude");
  const [read, write] = stub.calls;
  expect(read).toMatchObject({ method: "GET", path: "/api/engine-host" });
  expect(read!.headers.has(PERSON_HEADER)).toBe(false);
  expect(write).toMatchObject({ method: "POST", path: "/api/engine-host/claude/stop", body: JSON.stringify({ stop_background: false }) });
  expect(write!.headers.get(PERSON_HEADER)).toBe("mock-person-key");
});

it("Restart posts to /restart and Try again is the same request", async () => {
  const stub = service([]);
  await engineHostApi().retry("acp", { stopBackground: true });
  expect(stub.writes()[0]).toMatchObject({ method: "POST", path: "/api/engine-host/acp/restart", body: JSON.stringify({ stop_background: true }) });
  expect(stub.writes()[0]!.headers.get(PERSON_HEADER)).toBe("mock-person-key");
});

it("a refusal keeps its code and status and its words are cut", async () => {
  service([], [refusal(404, "not_found", "x".repeat(900))]);
  const failure = await engineHostApi().stop("nope").catch((error) => error);
  expect(failure).toMatchObject({ code: "not_found", status: 404 });
  expect(failure.message.length).toBeLessThanOrEqual(201);
});

it("Stop in Processes asks first, sends one write, and reads the list again", async () => {
  const stub = service([{ family: "claude", state: "running" }]);
  render(<Processes />);
  fireEvent.click(await screen.findByRole("button", { name: "Stop Claude Code engine host" }));
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Stop" }));
  await waitFor(() => expect(stub.writes()).toHaveLength(1));
  expect(stub.writes()[0]).toMatchObject({ path: "/api/engine-host/claude/stop", body: JSON.stringify({ stop_background: false }) });
  await waitFor(() => expect(stub.calls.filter((call) => call.method === "GET").length).toBeGreaterThanOrEqual(2));
});

it("a double click on Restart sends one request", async () => {
  const stub = service([{ family: "claude", state: "running" }]);
  render(<Processes />);
  const button = await screen.findByRole("button", { name: "Restart Claude Code engine host" });
  fireEvent.click(button);
  fireEvent.click(button);
  await waitFor(() => expect(stub.writes()).toHaveLength(1));
  expect(stub.writes()[0]).toMatchObject({ path: "/api/engine-host/claude/restart" });
});

it("a 409 background_open shows the question and sends again with stop_background only after the confirm", async () => {
  const words = "2 background tasks are still running. Stopping the host ends them.";
  const stub = service([{ family: "claude", state: "running" }], [refusal(409, "background_open", words), json({ family: "claude", state: "stopped", restart_count: 0 })]);
  render(<Processes />);
  fireEvent.click(await screen.findByRole("button", { name: "Stop Claude Code engine host" }));
  fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Stop" }));
  const ask = await screen.findByRole("dialog", { name: /anyway/ });
  expect(within(ask).getByText(words)).toBeVisible();
  // Nothing was sent again by itself.
  // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 50)); });
  expect(stub.writes()).toHaveLength(1);
  fireEvent.click(within(ask).getByRole("button", { name: "Stop anyway" }));
  await waitFor(() => expect(stub.writes()).toHaveLength(2));
  expect(stub.writes()[1]).toMatchObject({ path: "/api/engine-host/claude/stop", body: JSON.stringify({ stop_background: true }) });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
});

it("cancelling the background question sends nothing more", async () => {
  const stub = service([{ family: "claude", state: "running" }], [refusal(409, "background_open", "1 background task is still running. Stopping the host ends it.")]);
  render(<Processes />);
  fireEvent.click(await screen.findByRole("button", { name: "Restart Claude Code engine host" }));
  const ask = await screen.findByRole("dialog", { name: /anyway/ });
  fireEvent.click(within(ask).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(stub.writes()).toHaveLength(1);
});

it("a refusal is shown as plain capped text under the row", async () => {
  service([{ family: "claude", state: "running" }], [refusal(404, "not_found", `<b>No engine host is named that.</b>${"y".repeat(600)}`)]);
  render(<Processes />);
  fireEvent.click(await screen.findByRole("button", { name: "Restart Claude Code engine host" }));
  const alert = await screen.findByRole("alert");
  expect(alert.textContent!.length).toBeLessThanOrEqual(301);
  expect(alert.querySelector("b")).toBeNull();
});

it("the row is absent where no action makes sense: Stop only while restarting, Restart only when it can't start", async () => {
  service([{ family: "codex", state: "restarting" }, { family: "acp", state: "cant_start" }, { family: "muse", state: "stopped" }]);
  render(<Processes />);
  await screen.findAllByRole("listitem");
  expect(screen.queryByRole("button", { name: "Restart Codex engine host" })).toBeNull();
  expect(screen.getByRole("button", { name: "Stop Codex engine host" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Restart ACP agents engine host" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Stop ACP agents engine host" })).toBeNull();
});

it("Try again appears for a host that can't start, once per click, and not for a crashed one", async () => {
  const stub = service([{ family: "acp", state: "cant_start" }, { family: "codex", state: "crashed" }]);
  render(<EngineHostGroup />);
  const buttons = await screen.findAllByRole("button", { name: "Try again" });
  expect(buttons).toHaveLength(1);
  fireEvent.click(buttons[0]!);
  fireEvent.click(buttons[0]!);
  await waitFor(() => expect(stub.writes()).toHaveLength(1));
  expect(stub.writes()[0]).toMatchObject({ path: "/api/engine-host/acp/restart" });
});

it("the composer line's button restarts the host", async () => {
  const stub = service([]);
  const host = toHostRow({ family: "acp", state: "cant_start", restart_count: 5 })!;
  render(<ComposerHostLine line={{ host, engineName: "ACP" }} />);
  fireEvent.click(screen.getByRole("button", { name: "Restart engine host" }));
  await waitFor(() => expect(stub.writes()).toHaveLength(1));
  expect(stub.writes()[0]).toMatchObject({ path: "/api/engine-host/acp/restart" });
});
