import { render, screen, within } from "@testing-library/react";
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
const entry = (patch: Partial<ProviderEntry>): ProviderEntry => ({ id: "claude", name: "Claude Code", group: "in_use", labels: [], registered: true, ...patch });

function show(engines: EngineStatus[], providers: ProviderEntry[]) {
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
  const open = vi.fn();
  render(<ProviderTable store={store} onOpen={open} onEdit={() => undefined} engines={engines} catalog={{ providers }} />);
  return open;
}
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;

it("groups the list under the frame's headings and leaves out an empty group", () => {
  show([engine({}), engine({ id: "cursor", name: "Cursor", tier: "acp" })], [
    entry({}),
    entry({ id: "cursor", name: "Cursor", group: "found", labels: ["not_verified"] }),
    entry({ id: "pi", name: "Pi", group: "planned", labels: ["planned"], registered: false }),
  ]);
  expect(screen.getAllByRole("table").map((table) => table.getAttribute("aria-label"))).toEqual(["In use", "Found on this computer", "Not built yet"]);
  expect(screen.queryByText("Found, can’t start")).toBeNull();
});

it("says Not verified on a found engine and keeps its note", () => {
  show([engine({ id: "cursor", name: "Cursor", tier: "acp", auth: "not_applicable" })], [
    entry({ id: "cursor", name: "Cursor", group: "found", labels: ["not_verified"], note: "Starts over ACP. Not verified on a real install." }),
  ]);
  const row = rowOf("Cursor");
  expect(within(row).getByText("Not verified")).toBeVisible();
  expect(within(row).getByText("Starts over ACP. Not verified on a real install.")).toBeVisible();
});

it("says Unmanaged first and Not verified second on fx, as drawn", () => {
  show([engine({ id: "fx", name: "fx", tier: "acp", auth: "not_applicable" })], [
    entry({ id: "fx", name: "fx", group: "found", labels: ["unmanaged", "not_verified"] }),
  ]);
  const chips = within(rowOf("fx")).getAllByText(/^(Unmanaged|Not verified)$/).map((chip) => chip.textContent);
  expect(chips).toEqual(["Unmanaged", "Not verified"]);
});

it("says Limited support beside Ready on an older version", () => {
  show([engine({})], [entry({ labels: ["limited_support"], note: "Update to the supported version." })]);
  const row = rowOf("Claude Code");
  expect(within(row).getByText("Ready")).toBeVisible();
  expect(within(row).getByText("Limited support")).toBeVisible();
});

it("puts a known-broken engine under Found, can’t start and still opens it", async () => {
  const open = show([engine({})], [entry({ group: "cant_start", labels: ["cant_start"], note: "0.1.2 corrupts sessions." })]);
  expect(screen.getByRole("table", { name: "Found, can’t start" })).toBeVisible();
  await userEvent.click(screen.getByRole("button", { name: "Claude Code" }));
  expect(open).toHaveBeenCalledWith("claude");
});

it("lists an installed app with no ACP server as Can’t start with nothing to open or switch", () => {
  show([], [entry({ id: "antigravity", name: "Antigravity", group: "cant_start", labels: ["cant_start"], registered: false,
    note: "Antigravity.app is installed, but it has no ACP server Orbyn can start." })]);
  const row = rowOf("Antigravity");
  expect(within(row).getByText("Can’t start")).toBeVisible();
  expect(within(row).queryByRole("button")).toBeNull();
  expect(within(row).queryByRole("switch")).toBeNull();
});

it("lists an engine that is not built as Planned, with no action", () => {
  show([], [entry({ id: "pi", name: "Pi", group: "planned", labels: ["planned"], registered: false, note: "Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet." })]);
  expect(screen.getByRole("table", { name: "Not built yet" })).toBeVisible();
  const row = rowOf("Pi");
  expect(within(row).getByText("Planned")).toBeVisible();
  expect(within(row).getByText("Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet.")).toBeVisible();
  expect(within(row).queryByRole("button")).toBeNull();
});

it("falls back to one flat table of the engines until the service answers", () => {
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
  render(<ProviderTable store={store} onOpen={() => undefined} onEdit={() => undefined} engines={[engine({})]} />);
  expect(screen.getByRole("table", { name: "Providers" })).toBeVisible();
});
