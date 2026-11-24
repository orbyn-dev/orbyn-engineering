import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus, JsonValue } from "../../../api/types";
import type { ScopedPreferencesChange } from "../../../generated/ScopedPreferencesChange";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { wire, wireWrites } from "../../../wire.test-support";
import { seedPreference } from "../rows/seedPreference.test-support";
import { ProvidersSection } from "./Providers";

const base = { capabilities: { managed: true, approvals: true }, installed: true, version: "2.1.318", auth: "logged_in", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null } as const;
const engine = (patch: Partial<EngineStatus>) => ({ ...base, ...patch }) as unknown as EngineStatus;
const engines = () => [
  engine({ id: "claude", name: "Claude Code", tier: "native", path: "/opt/claude" }),
  engine({ id: "work", name: "Claude Code · Work", tier: "native", path: "/opt/work", instance_of: "claude" }),
  engine({ id: "acp-base", name: "Agent", tier: "acp", path: "/opt/agent" }),
];
let mock: MockApi;
/** The service reports these engines, and the page has read them. */
async function report(list: EngineStatus[]) {
  mock.setEngines(list);
  await useWorkspace.getState().refreshEngines();
}
/** What the service holds for the person's provider instances, with the catalogue version and revision a save must start from. */
const read = () => mock.handle("POST", "/api/settings/preferences/query", { scope: {}, keys: ["provider_instances"] }).body as ScopedPreferencesResult;
const entries = () => read().rows[0]?.value as JsonValue[];
const changes = () => wireWrites().filter((call) => call.method === "PUT" && call.path === "/api/settings/preferences").map((call) => call.body as ScopedPreferencesChange);
const instanceWrites = () => changes().filter((change) => change.edits.provider_instances);
const inUseWrites = () => changes().flatMap((change) => change.edits.disabled_engines ? [(change.edits.disabled_engines as { value: JsonValue }).value] : []);
const probes = () => wire().filter((call) => call.path === "/api/engines/probe-binary");
// Reading the list is its own request, and the page replaces its rows when it arrives: act once the groups are drawn.
const settled = () => screen.findByRole("rowgroup", { name: "In use" });

beforeEach(async () => {
  mock = installMockFetch();
  seedPreference(mock, "provider_instances", [{ id: "work", driver: "claude", name: "Work", enabled: false, forward: "mine", program: "/opt/work" }, { id: "other", driver: "acp-base", enabled: true, forward: { nested: true } }]);
  seedPreference(mock, "disabled_engines", []);
  await report(engines());
});

describe("provider instance dialog", () => {
  it("keeps Save off until something changes, then saves through environment CAS and keeps peers", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    await settled();
    await user.click(await screen.findByRole("button", { name: "Edit Claude Code · Work" }));
    const dialog = await screen.findByRole("dialog", { name: /Edit/ });
    const held = read();
    const save = within(dialog).getByRole("button", { name: "Save" });
    expect(save).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    const name = within(dialog).getByRole("textbox", { name: "Display name" });
    await user.clear(name); await user.type(name, "Work renamed");
    await user.click(within(dialog).getByRole("button", { name: "Colour #3b82f6" }));
    await user.click(save);
    await waitFor(() => expect(instanceWrites()).toHaveLength(1));
    expect(instanceWrites()[0]).toMatchObject({ scope: {}, membership_version: held.catalog.version, if_match: { provider_instances: held.rows[0]?.revision } });
    expect(entries()).toEqual([{ id: "work", driver: "claude", name: "Work renamed", accent: "#3b82f6", enabled: false, forward: "mine", program: "/opt/work" }, { id: "other", driver: "acp-base", enabled: true, forward: { nested: true } }]);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
  it("saves a Claude auto-compact window only inside its range", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    await settled();
    await user.click(await screen.findByRole("button", { name: "Edit Claude Code · Work" }));
    const dialog = await screen.findByRole("dialog");
    const field = within(dialog).getByRole("textbox", { name: "Auto-compact window" });
    await user.type(field, "99999");
    expect(within(dialog).getByText("Use 100000 to 1000000.")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
    await user.clear(field); await user.type(field, "300000");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(entries()[0]).toMatchObject({ auto_compact_window: "300000" }));
  });
  it("names the reason under the path when the save is refused and leaves the list alone", async () => {
    const user = userEvent.setup();
    render(<ProvidersSection />);
    await settled();
    await user.click(await screen.findByRole("button", { name: "Edit Claude Code · Work" }));
    const dialog = await screen.findByRole("dialog");
    const path = within(dialog).getByRole("textbox", { name: "Binary path" });
    await user.clear(path); await user.type(path, "/opt/claude");
    const before = entries();
    mock.fail("/api/settings/preferences", 1, 400, "invalid_input", "work names an empty program.");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(dialog).findByText("work names an empty program.")).toBeVisible();
    expect(entries()).toEqual(before);
    expect(path).toBeInvalid();
    expect(within(dialog).getByRole("button", { name: "Save" })).toBeEnabled();
  });
  const openPath = async (user: ReturnType<typeof userEvent.setup>) => {
    render(<ProvidersSection />);
    await settled();
    await user.click(await screen.findByRole("button", { name: "Edit Claude Code · Work" }));
    const dialog = await screen.findByRole("dialog");
    return { dialog, path: within(dialog).getByRole("textbox", { name: "Binary path" }), save: () => within(dialog).getByRole("button", { name: "Save" }) };
  };
  it("runs the program check when you leave the path field, not while you type", async () => {
    const user = userEvent.setup();
    const { dialog, path } = await openPath(user);
    await user.clear(path); await user.type(path, "/opt/claude");
    expect(probes()).toHaveLength(0);
    await user.tab();
    expect(await within(dialog).findByText("Found. Version 2.1.318.")).toBeVisible();
    expect(probes()).toEqual([{ method: "POST", path: "/api/engines/probe-binary", body: { path: "/opt/claude" } }]);
  });
  it("saves a bare program name the engines find on PATH", async () => {
    const user = userEvent.setup();
    const { dialog, path, save } = await openPath(user);
    await user.clear(path); await user.type(path, "my-agent"); await user.tab();
    expect(await within(dialog).findByText("Give the full path to the program.")).toBeVisible();
    expect(save()).toBeEnabled();
  });
  it("blocks Save on an absolute path with no file", async () => {
    const user = userEvent.setup();
    const { dialog, path, save } = await openPath(user);
    await user.clear(path); await user.type(path, "/opt/claud"); await user.tab();
    expect(await within(dialog).findByText("No file at this path.")).toBeVisible();
    expect(save()).toBeDisabled();
  });
  it("does not block Save when the program exists but its version check fails", async () => {
    const user = userEvent.setup();
    const { dialog, path, save } = await openPath(user);
    await user.clear(path); await user.type(path, "/opt/broken"); await user.tab();
    expect(await within(dialog).findByText("Its version command failed.")).toBeVisible();
    expect(save()).toBeEnabled();
  });
  it("turns a provider in use on or off for the scope, through the providers-in-use preference", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    const toggle = await screen.findByRole("switch", { name: "Use Claude Code · Work" });
    await waitFor(() => expect(toggle).toBeEnabled());
    await user.click(toggle);
    await waitFor(() => expect(inUseWrites()).toEqual([["work"]]));
  });
  it("offers Edit on instances only", async () => {
    render(<ProvidersSection />);
    await settled();
    await screen.findByRole("button", { name: "Edit Claude Code · Work" });
    expect(screen.queryByRole("button", { name: "Edit Claude Code" })).toBeNull();
  });
});

describe("provider pages", () => {
  it("adds an instance from what was found, with the path filled in", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    await settled();
    await user.click(await screen.findByRole("button", { name: "Add provider" }));
    const choose = await screen.findByRole("dialog", { name: "Add a provider" });
    expect(within(choose).getByText("Found at /opt/claude · 2.1.318")).toBeVisible();
    await user.click(within(choose).getByRole("button", { name: "Continue" }));
    const editor = await screen.findByRole("dialog", { name: "Add Claude Code" });
    expect(within(editor).getByRole("textbox", { name: "Binary path" })).toHaveValue("/opt/claude");
    await user.type(within(editor).getByRole("textbox", { name: "Display name" }), "Personal");
    await user.click(within(editor).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(entries()).toHaveLength(3));
    expect(entries()[2]).toMatchObject({ id: "claude_personal", driver: "claude", name: "Personal", program: "/opt/claude", enabled: true });
  });
  it("names who is signed in under each provider, and the instance beside it", async () => {
    await report([engine({ id: "claude", name: "Claude Code", tier: "native", account: "sam@example.com" }),
      engine({ id: "work", name: "Claude Code · Work", tier: "native", instance_of: "claude", account: "sam@acme.com" }),
      engine({ id: "codex", name: "Codex", tier: "native", auth: "logged_out" })]);
    render(<ProvidersSection />);
    await screen.findByRole("rowgroup", { name: "In use" });
    expect(await screen.findByText(/Runs through Orbyn’s engine host with the Claude Agent SDK/)).toBeVisible();
    expect(screen.getByText("Work · sam@acme.com")).toBeVisible();
    expect(screen.getByText("Not signed in")).toBeVisible();
  });
  it("shows the signed-in account on the provider's page", async () => {
    const user = userEvent.setup();
    await report([engine({ id: "claude", name: "Claude Code", tier: "native", account: "sam@example.com" })]);
    render(<ProvidersSection />);
    await screen.findByRole("rowgroup", { name: "In use" });
    await user.click(await screen.findByRole("button", { name: "Claude (SDK)" }));
    expect(await screen.findByText("Signed in as sam@example.com")).toBeVisible();
  });
  it("offers Review with the count of sessions the service finds, and opens the history import", async () => {
    const user = userEvent.setup();
    await report([engine({ id: "claude", name: "Claude Code", tier: "native" }), engine({ id: "codex", name: "Codex", tier: "native" })]);
    mock.setNativeSessionCount("claude", 12);
    mock.setNativeSessionCount("codex", 6);
    render(<ProvidersSection />);
    expect(await screen.findByText("Provider sessions this computer keeps")).toBeVisible();
    await user.click(await screen.findByRole("button", { name: "Review 18" }));
    expect(await screen.findByRole("button", { name: "Providers" })).toBeVisible();
  });
  it("marks the count as a lower bound when the history folder was too large to count to the end", async () => {
    await report([engine({ id: "claude", name: "Claude Code", tier: "native" }), engine({ id: "codex", name: "Codex", tier: "native" })]);
    mock.setNativeSessionCount("claude", 12, true);
    mock.setNativeSessionCount("codex", 0);
    render(<ProvidersSection />);
    expect(await screen.findByRole("button", { name: "Review 12+" })).toBeVisible();
  });
  it("still shows Native sessions when only one engine fails to count", async () => {
    await report([engine({ id: "claude", name: "Claude Code", tier: "native" }), engine({ id: "codex", name: "Codex", tier: "native" })]);
    mock.setNativeSessionCount("claude", 12);
    mock.fail("/api/engines/codex/native-sessions/count", 99, 422, "unsupported", "This engine cannot count its sessions.");
    render(<ProvidersSection />);
    expect(await screen.findByRole("button", { name: "Review 12" })).toBeVisible();
  });
  it("hides Native sessions when the service has no count to give", async () => {
    for (const id of ["claude", "work", "acp-base"]) mock.fail(`/api/engines/${id}/native-sessions/count`, 99, 422, "unsupported", "This engine cannot count its sessions.");
    render(<ProvidersSection />);
    await screen.findByRole("rowgroup", { name: "In use" });
    expect(screen.queryByText("Native sessions")).toBeNull();
  });
  it("opens a provider and returns with the back link", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    await screen.findByRole("rowgroup", { name: "In use" });
    await user.click(await screen.findByRole("button", { name: "Claude Code · Work" }));
    expect(await screen.findByRole("heading", { name: "Claude Code · Work" })).toBeVisible();
    expect(screen.getByText("/opt/work")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Providers" }));
    expect(await screen.findByRole("rowgroup", { name: "In use" })).toBeVisible();
  });
  it("removes only the selected instance record", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    await screen.findByRole("rowgroup", { name: "In use" });
    await user.click(await screen.findByRole("button", { name: "Claude Code · Work" }));
    await user.click(await screen.findByRole("button", { name: "Remove Claude Code · Work" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(entries()).toEqual([{ id: "other", driver: "acp-base", enabled: true, forward: { nested: true } }]));
  });
  it("opens a provider row from the keyboard and leaves a row with no detail behind it without a control", async () => {
    const user = userEvent.setup(); render(<ProvidersSection />);
    await screen.findByRole("rowgroup", { name: "In use" });
    // The row's name button is its control: it is tabbable, it carries the designs' disclosure chevron, and Enter opens.
    const name = await screen.findByRole("button", { name: "Claude Code · Work" });
    expect(name.querySelector("svg")).not.toBeNull();
    name.focus();
    expect(name).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(await screen.findByRole("heading", { name: "Claude Code · Work" })).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Providers" }));
    // A table row is not a control: it takes no tabindex of its own, so only the name button is tabbable.
    const row = (await screen.findByRole("button", { name: "Claude Code · Work" })).closest('[role="row"]');
    expect(row).not.toBeNull();
    expect(row).not.toHaveAttribute("tabindex");
    // The ACP provider has no detail page to open, so its row offers no control at all.
    expect(screen.queryByRole("button", { name: "Agent" })).toBeNull();
  });
});
