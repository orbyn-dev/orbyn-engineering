import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import type { EngineStatus } from "../../../api/types";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import { ProviderTable } from "./ProviderTable";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: vi.fn(), reload: vi.fn() };
const engine = (patch: Partial<EngineStatus>): EngineStatus => ({
  id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318", auth: "logged_in",
  latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null, ...patch,
} as EngineStatus);
const entry = (patch: Partial<ProviderEntry>): ProviderEntry => ({ id: "claude", name: "Claude Code", group: "in_use", labels: [], registered: true, openable: true, ...patch });

/** What the per-project switch holds: the engines turned off here. */
function show(engines: EngineStatus[], providers: ProviderEntry[], off: string[] = []) {
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [{ key: "disabled_engines", revision: "r", value: off, mixed: false, default: [], project_scoped: true,
    environment: { source: "default", value: off, default: [] }, members: [], overrides: [] }] } as never);
  const open = vi.fn();
  render(<ProviderTable store={store} onOpen={open} onEdit={() => undefined} engines={engines} catalog={{ providers }} />);
  return open;
}
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;

const switches = () => screen.findAllByRole("switch");
const tables = () => within(screen.getByRole("table", { name: "Providers" })).getAllByRole("rowgroup").map((group) => group.getAttribute("aria-label"));

it("groups the list under the frame's headings by the in-use switch, in one table with one set of column headers", async () => {
  show([engine({}), engine({ id: "cursor", name: "Cursor", tier: "acp" })], [
    entry({}),
    entry({ id: "cursor", name: "Cursor", group: "found", labels: ["not_verified"], openable: false }),
    entry({ id: "pi", name: "Pi", group: "planned", labels: ["planned"], registered: false, openable: false }),
  ], ["cursor"]);
  await switches();
  await waitFor(() => expect(tables()).toEqual(["In use", "Found on this computer", "Not built yet"]));
  // One table, one set of column headers, the groups as row groups inside it.
  expect(screen.getAllByRole("table")).toHaveLength(1);
  expect(screen.getAllByRole("columnheader", { name: "Provider" })).toHaveLength(1);
  expect(screen.queryByText("Found, can’t start")).toBeNull();
});

it("puts an engine switched on under In use even when it is not verified, and says so", async () => {
  show([engine({ id: "cursor", name: "Cursor", tier: "acp", auth: "not_applicable" })], [
    entry({ id: "cursor", name: "Cursor", group: "found", labels: ["not_verified"], note: "Starts over ACP. Not verified on a real install.", openable: false }),
  ]);
  await switches();
  expect(tables()).toEqual(["In use"]);
  const row = rowOf("Cursor");
  expect(within(row).getByText("Not verified")).toBeVisible();
  expect(within(row).getByText(/Starts over ACP\. Not verified on a real install\./)).toBeVisible();
  // Cursor has no detail page yet: its row carries no Open and its switch stays.
  expect(within(row).queryByRole("button", { name: "Cursor" })).toBeNull();
  expect(within(row).getByRole("switch", { name: "Use Cursor" })).toBeVisible();
});

it("says Unmanaged first and Not verified second on fx, as drawn", async () => {
  show([engine({ id: "fx", name: "fx", tier: "acp", auth: "not_applicable" })], [
    entry({ id: "fx", name: "fx", group: "found", labels: ["unmanaged", "not_verified"], openable: false }),
  ], ["fx"]);
  await waitFor(() => expect(tables()).toEqual(["Found on this computer"]));
  const chips = within(rowOf("fx")).getAllByText(/^(Unmanaged|Not verified)$/).map((chip) => chip.textContent);
  expect(chips).toEqual(["Unmanaged", "Not verified"]);
});

it("lists the other unverified engines under More engines with their note and program name", async () => {
  show([engine({ id: "cline", name: "Cline", tier: "acp", auth: "not_applicable" })], [
    entry({ id: "cline", name: "Cline", group: "more", labels: ["not_verified", "planned"], note: "cline · its effort flag is not built" }),
  ], ["cline"]);
  await waitFor(() => expect(tables()).toEqual(["More engines"]));
  expect(screen.getByText(/These start over ACP when Orbyn finds their program/)).toBeVisible();
  const row = rowOf("Cline");
  expect(within(row).getByText("cline · its effort flag is not built")).toBeVisible();
  expect(within(row).getAllByText(/^(Not verified|Planned)$/).map((chip) => chip.textContent)).toEqual(["Not verified", "Planned"]);
});

it("says Limited support beside Ready on an older version", async () => {
  show([engine({})], [entry({ labels: ["limited_support"], note: "Update to the supported version." })]);
  await switches();
  const row = rowOf("Claude Code");
  expect(within(row).getByText("Ready")).toBeVisible();
  expect(within(row).getByText("Limited support")).toBeVisible();
});

it("puts a known-broken engine under Found, can’t start and still opens it", async () => {
  const open = show([engine({})], [entry({ group: "cant_start", labels: ["cant_start"], note: "0.1.2 corrupts sessions." })]);
  await switches();
  expect(tables()).toEqual(["Found, can’t start"]);
  await userEvent.click(screen.getByRole("button", { name: "Claude Code" }));
  expect(open).toHaveBeenCalledWith("claude");
});

it("keeps an engine whose program was not found, with its switch and Edit", async () => {
  const instance = engine({ id: "mine", name: "Claude Code · Mine", instance_of: "claude", installed: false, path: null, version: null });
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
  render(<ProviderTable store={{ ...store, instances: [{ id: "mine", driver: "claude", enabled: true }] }} onOpen={() => undefined} onEdit={() => undefined} engines={[instance]}
    catalog={{ providers: [entry({ id: "mine", name: "Claude Code · Mine", group: "not_found", labels: ["not_found"] })] }} />);
  const row = rowOf("Claude Code");
  expect(tables()).toEqual(["Not found"]);
  expect(within(row).getAllByText("Not found")).toHaveLength(1);
  expect(within(row).getByRole("button", { name: "Edit Claude Code · Mine" })).toBeVisible();
  expect(within(row).getByRole("button", { name: "Claude Code · Mine" })).toBeVisible();
});

it("shows a row for an engine the list has not heard of yet, and keeps the instance's name beside the note", async () => {
  show([
    engine({ id: "work", name: "Claude Code · Work", instance_of: "claude", account: "sam@acme.com" }),
    engine({ id: "fresh", name: "Fresh", tier: "acp", auth: "not_applicable" }),
  ], [entry({ id: "work", name: "Claude Code · Work", labels: ["limited_support"], note: "Update to the supported version." })]);
  await switches();
  expect(rowOf("Fresh")).toBeVisible();
  expect(within(rowOf("Fresh")).queryByRole("button", { name: "Fresh" })).toBeNull();
  const sub = within(rowOf("Claude Code")).getByText(/Update to the supported version\./);
  expect(sub.textContent).toContain("Work");
});

it("keeps a non-beta detail closed even when older catalog data says it opens", async () => {
  show([engine({ id: "cline", name: "Cline", tier: "acp", auth: "not_applicable" })], [
    entry({ id: "cline", name: "Cline", group: "more", openable: true, labels: ["not_verified"] }),
  ]);
  await switches();
  expect(within(rowOf("Cline")).queryByRole("button", { name: "Cline" })).toBeNull();
  expect(within(rowOf("Cline")).getByRole("switch", { name: "Use Cline" })).toBeVisible();
});

it("shows Signed out for a signed-out engine even with no sign-in command", async () => {
  show([engine({ auth: "logged_out", sign_in_command: null })], [entry({})]);
  await switches();
  expect(within(rowOf("Claude Code")).getByText("Signed out")).toBeVisible();
  expect(within(rowOf("Claude Code")).queryByText("Ready")).toBeNull();
});

it("lists a refused program as a card that says so, with no action", async () => {
  show([], [entry({ id: "grok", name: "Grok Build", group: "not_found", labels: ["not_found"], registered: false, openable: false,
    note: "Found a program named grok at /usr/local/bin/grok, but it is not xAI’s Grok CLI. Orbyn does not use it." })]);
  await waitFor(() => expect(tables()).toEqual(["Not found"]));
  const row = rowOf("Grok Build");
  expect(within(row).getByText("Not found")).toBeVisible();
  expect(within(row).getByText(/Found a program named grok/)).toBeVisible();
  expect(within(row).queryByRole("button")).toBeNull();
});

it("lists an installed app with no ACP server as Can’t start with nothing to open or switch", async () => {
  show([], [entry({ id: "antigravity", name: "Antigravity", group: "cant_start", labels: ["cant_start"], registered: false, openable: false,
    note: "Antigravity.app is installed, but it has no ACP server Orbyn can start." })]);
  await waitFor(() => expect(tables()).toEqual(["Found, can’t start"]));
  const row = rowOf("Antigravity");
  expect(within(row).getByText("Can’t start")).toBeVisible();
  expect(within(row).queryByRole("button")).toBeNull();
  expect(within(row).queryByRole("switch")).toBeNull();
});

it("lists an engine that is not built as Planned, with no action", async () => {
  show([], [entry({ id: "pi", name: "Pi", group: "planned", labels: ["planned"], registered: false, openable: false, note: "Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet." })]);
  await waitFor(() => expect(tables()).toEqual(["Not built yet"]));
  const row = rowOf("Pi");
  expect(within(row).getByText("Planned")).toBeVisible();
  expect(within(row).getByText("Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet.")).toBeVisible();
  expect(within(row).queryByRole("button")).toBeNull();
});

it("falls back to one flat table of the engines until the service answers", () => {
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
  render(<ProviderTable store={store} onOpen={() => undefined} onEdit={() => undefined} engines={[engine({})]} />);
  expect(screen.getByRole("table", { name: "Providers" })).toBeVisible();
  expect(screen.queryAllByRole("rowgroup")).toHaveLength(0);
});

it("opens beta instances but keeps other families closed before catalog loading", async () => {
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
  const open = vi.fn();
  render(<ProviderTable store={store} onOpen={open} onEdit={() => undefined} engines={[
    engine({ id: "work", name: "OpenCode · Work", instance_of: "opencode", tier: "acp" }),
    engine({ id: "cline", name: "Cline", tier: "acp" }),
  ]} />);
  await userEvent.click(screen.getByRole("button", { name: "OpenCode · Work" }));
  expect(open).toHaveBeenCalledExactlyOnceWith("work");
  expect(screen.queryByRole("button", { name: "Cline" })).toBeNull();
});

it("writes the program alone as an ACP row's line, with Unmanaged for the ones Orbyn cannot ask", async () => {
  const acp = (id: string, name: string): EngineStatus => engine({ id, name, tier: "acp", installed: true, auth: "not_applicable", path: `/usr/local/bin/${id}`, version: null });
  show([acp("kiro", "Kiro"), acp("kimi", "Kimi Code")], [
    entry({ id: "kiro", name: "Kiro", group: "more", labels: ["unmanaged", "not_verified"], note: "kiro-cli", openable: false }),
    entry({ id: "kimi", name: "Kimi Code", group: "more", labels: ["not_verified"], note: "kimi", openable: false }),
  ], ["kiro", "kimi"]);

  await waitFor(() => expect(tables()).toEqual(["More engines"]));
  const kiro = rowOf("Kiro");
  expect(within(kiro).getByText("kiro-cli")).toBeVisible();
  expect(within(kiro).queryByText("via ACP")).toBeNull();
  expect(within(kiro).getByText("Unmanaged")).toBeVisible();
  expect(within(kiro).getByText("Not verified")).toBeVisible();
  const kimi = rowOf("Kimi Code");
  expect(within(kimi).getByText("kimi")).toBeVisible();
  expect(within(kimi).queryByText("Unmanaged")).toBeNull();
});

it("lists the six ACP engines not installed as via ACP and Not found alone, with nothing to open (frame acp-status-list-not-installed-narrow)", async () => {
  const acp = (id: string, name: string): EngineStatus => engine({ id, name, tier: "acp", installed: false, auth: "not_applicable", path: null, version: null });
  const rows = [["kiro", "Kiro"], ["droid", "Factory Droid"], ["omp", "Oh My Pi"], ["mistral-vibe", "Mistral Vibe"], ["glm", "GLM"], ["kimi", "Kimi Code"]] as const;
  show(rows.map(([id, name]) => acp(id, name)), rows.map(([id, name]) => entry({ id, name, group: "not_found", labels: ["not_found"], registered: true, openable: false })));

  await waitFor(() => expect(tables()).toEqual(["Not found"]));
  for (const [, name] of rows) {
    const row = rowOf(name);
    expect(within(row).getByText("via ACP")).toBeVisible();
    expect(within(row).getAllByText("Not found")).toHaveLength(1);
    expect(within(row).queryByText("Unmanaged")).toBeNull();
    expect(within(row).queryByText("Not verified")).toBeNull();
    expect(within(row).queryByText("kiro-cli")).toBeNull();
    expect(within(row).queryByRole("button", { name })).toBeNull();
    expect(within(row).getByRole("switch", { name: `Use ${name}` })).toBeVisible();
    expect(row.querySelector(`[aria-label="Use ${name}"]`)).not.toBeNull();
  }
});

it("shows a failed start in the Status cell instead of Ready, once, with the service's straight apostrophe kept", async () => {
  show([engine({ id: "kiro", name: "Kiro", tier: "acp", auth: "not_applicable", start_issue: { kind: "failed", message: "Couldn't run Kiro. It exited.", at: "2026-11-22T10:00:00Z" } })], [
    entry({ id: "kiro", name: "Kiro", group: "found", labels: ["start_failed"], note: "Couldn't run Kiro. It exited.", openable: false }),
  ]);
  await switches();
  const row = rowOf("Kiro");
  expect(within(row).getAllByText("Start failed")).toHaveLength(1);
  expect(within(row).queryByText("Ready")).toBeNull();
  expect(within(row).getByText("Couldn't run Kiro. It exited.")).toBeVisible();
});

it("says Signed out once for a signed-out ACP engine, with its sign-in command", async () => {
  show([engine({ id: "droid", name: "Factory Droid", tier: "acp", auth: "logged_out", sign_in_command: "droid login", start_issue: { kind: "signed_out", message: "Factory Droid isn't signed in. Run droid login, then try again.", at: "2026-11-22T10:00:00Z" } })], [
    entry({ id: "droid", name: "Factory Droid", group: "found", labels: ["signed_out"], openable: false }),
  ]);
  await switches();
  const row = rowOf("Factory Droid");
  expect(within(row).getAllByText("Signed out")).toHaveLength(1);
  expect(within(row).getByText("droid login")).toBeVisible();
  expect(within(row).queryByText("Ready")).toBeNull();
});
