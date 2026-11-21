import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createEngineHostMock, hostFixtures } from "../../../../mock/engineHost";
import { CauseChain } from "./HostParts";
import { EngineHostBanner, EngineHostCard } from "./HostCard";
import { EngineHostGroup } from "./HostGroup";
import { setEngineHostApi, type HostRow } from "../../enginehost";

afterEach(() => { setEngineHostApi(null); vi.unstubAllGlobals(); });
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;

it("draws nothing, and no error, while the service answers 404 for the engine host", async () => {
  const fetching = vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404, headers: { "content-type": "application/json" } }));
  vi.stubGlobal("fetch", fetching);
  render(<><EngineHostGroup /><EngineHostCard engineId="codex" /><EngineHostBanner engineId="codex" /></>);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Engine host")).toBeNull();
  expect(screen.queryByRole("table", { name: "Engine host" })).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(fetching.mock.calls.every(([url, init]) => url === "/api/engine-host" && init!.method === "GET")).toBe(true);
});

it("lists one row per host with its chip and its line, as the frame does", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<EngineHostGroup />);
  const table = await screen.findByRole("table", { name: "Engine host" });
  expect(within(table).getAllByRole("row").filter((row) => row.querySelector(".name"))).toHaveLength(8);
  expect(screen.getByText("One helper program runs each engine family. Orbyn starts it, stops it when it is idle and restarts it if it crashes.")).toBeVisible();
  expect(within(rowOf("Claude Code · Personal")).getByText("Running")).toBeVisible();
  expect(within(rowOf("Claude Code · Personal")).getByText("Host 0.1.0 · protocol 1 · since 09:12")).toBeVisible();
  expect(within(rowOf("Codex")).getByText("Restart 3 of 5 · next try in 4 s")).toBeVisible();
  expect(within(rowOf("Codex")).getByText("Restarting")).toBeVisible();
  expect(within(rowOf("Pi")).getByText("Version mismatch")).toBeVisible();
  expect(within(rowOf("Pi")).getByText("The engine host does not match this app version. Host protocol 2, app protocol 1.")).toBeVisible();
  // Try again is the only action, and only on the family that waits for the person.
  expect(screen.getAllByRole("button", { name: /try again/i })).toHaveLength(1);
  expect(within(rowOf("ACP agents")).getByRole("button", { name: "Try again" })).toBeVisible();
});

it("writes once for one Try again click, however fast the clicks come", async () => {
  const mock = createEngineHostMock({ delayMs: 20 });
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<EngineHostGroup />);
  const button = await within(await screen.findByRole("table", { name: "Engine host" })).findByRole("button", { name: "Try again" });
  await user.dblClick(button);
  await waitFor(() => expect(within(rowOf("ACP agents")).getByText("Starting")).toBeVisible());
  expect(mock.writes).toEqual(["retry:acp"]);
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
});

it("says why a Try again failed on the row and keeps the button", async () => {
  const mock = createEngineHostMock();
  mock.retry = async () => { throw new Error("The host binary is missing."); };
  setEngineHostApi(mock);
  const user = userEvent.setup();
  render(<EngineHostGroup />);
  await user.click(await screen.findByRole("button", { name: "Try again" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The host binary is missing.");
  expect(screen.getByRole("button", { name: "Try again" })).toBeEnabled();
});

it("shows the running card: version, protocol, SDK and what the host can do", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<EngineHostCard engineId="claude" />);
  const card = await screen.findByRole("group", { name: "Engine host" });
  expect(within(card).getByText("Running")).toBeVisible();
  expect(within(card).getByText("orbyn-engine-host 0.1.0")).toBeVisible();
  expect(within(card).getByText(/protocol 1/)).toBeVisible();
  expect(within(card).getByText("@anthropic-ai/claude-agent-sdk")).toBeVisible();
  expect(within(card).getByText("Steer a run, fork, background work, roll back, list models, report usage")).toBeVisible();
});

it("shows the restarting card with its last error closed, then the cause chain without a stack", async () => {
  const rows = hostFixtures().map((row) => (row.id === "codex" ? { ...row, last_error: { ...row.last_error!, stack: "at secret (host.mjs:1)" } as never } : row));
  setEngineHostApi(createEngineHostMock({ hosts: rows }));
  const user = userEvent.setup();
  render(<><EngineHostBanner engineId="codex" /><EngineHostCard engineId="codex" /></>);
  expect(await screen.findByRole("alert")).toHaveTextContent("The Codex engine host stopped at 09:41. 2 threads that were running on it failed. Your work in the project is kept.");
  expect(screen.getByText("Restart 3 of 5 · next try in 4 s")).toBeVisible();
  expect(screen.getByText("The engine host closed its output")).toBeVisible();
  expect(screen.queryByText("EPIPE")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Show details" }));
  expect(screen.getByText("Kind: host")).toBeVisible();
  expect(screen.getByText("EPIPE")).toBeVisible();
  expect(screen.queryByText(/host\.mjs/)).toBeNull();
  await user.click(screen.getByRole("button", { name: "Hide details" }));
  expect(screen.queryByText("EPIPE")).toBeNull();
});

it("shows a mismatch with both protocol numbers and no button to fix it", async () => {
  setEngineHostApi(createEngineHostMock());
  render(<><EngineHostBanner engineId="pi" /><EngineHostCard engineId="pi" /></>);
  expect(await screen.findByRole("alert")).toHaveTextContent("The host speaks protocol 2; this app speaks protocol 1.");
  expect(screen.getByText("Refused at hello. Nothing is running on it.")).toBeVisible();
  expect(screen.getByText("orbyn-engine-host 0.2.0")).toBeVisible();
  expect(screen.getByText("Protocol 1")).toBeVisible();
  expect(screen.queryByRole("button")).toBeNull();
});

it("shows Not used with its own sentence", async () => {
  const rows: HostRow[] = [{ id: "claude", name: "Claude Code", state: "not_used", can_retry: false }];
  setEngineHostApi(createEngineHostMock({ hosts: rows }));
  render(<EngineHostCard engineId="claude" />);
  expect(await screen.findByText("Not used. Claude Code keeps running through its own program.")).toBeVisible();
});

it("numbers every cause outermost first and ends on the loop marker in plain words", () => {
  const causes = [{ name: "SessionError", message: "could not start the turn" }, { name: "Error", message: "read ECONNRESET", code: "ECONNRESET" },
    { name: "Error", message: "[Circular error cause]" }];
  render(<CauseChain kind="network" causes={causes} />);
  const items = screen.getAllByRole("listitem");
  expect(items.map((item) => item.textContent)).toEqual(["1SessionError could not start the turn", "2Error read ECONNRESET ECONNRESET", "3[Circular error cause]"]);
  expect(screen.getByText("Kind: network")).toBeVisible();
});

beforeEach(() => vi.useRealTimers());
