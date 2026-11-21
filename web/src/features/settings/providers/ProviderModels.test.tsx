import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import type { EngineStatus } from "../../../api/types";
import { installMockFetch } from "../../../test-support";
import { useWorkspace } from "../../../product/state/workspace";
import { ProviderDetail } from "./ProviderDetail";

const engine = { id: "claude", name: "Claude Code", tier: "native", installed: true, version: "2.1.318", latest: null, auth: "logged_in", path: "/opt/claude",
  capabilities: {}, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null } as unknown as EngineStatus;
const store = { instances: [], raw: [], disabled: false, save: async () => null } as never;

it("limits the OpenCode detail page to its reported facts and models", async () => {
  installMockFetch();
  const opencode = { ...engine, id: "opencode", name: "OpenCode", tier: "acp", path: "/opt/opencode", version: "2.2.5", support: "unknown" } as EngineStatus;
  render(<ProviderDetail engine={opencode} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  expect(screen.getByText("2.2.5")).toBeVisible();
  expect(screen.getByText("/opt/opencode")).toBeVisible();
  expect(screen.getByText("Models", { selector: "p" })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Remove OpenCode" })).toBeNull();
  expect(screen.getByText("Crew tier mapping")).toBeVisible();
  expect(screen.getByRole("button", { name: "Open Settings › Crew" })).toBeDisabled();
  expect(screen.getByText("Planned")).toBeVisible();
});

it("keeps Remove on a named OpenCode instance, but not on the built-in OpenCode", async () => {
  installMockFetch();
  const mine = { ...engine, id: "opencode-work", instance_of: "opencode", name: "OpenCode work", tier: "acp", path: "/opt/opencode", version: "2.2.5", support: "unknown" } as EngineStatus;
  const withInstance = { instances: [{ id: "opencode-work" }], raw: [{ id: "opencode-work" }], disabled: false, save: async () => null } as never;
  render(<ProviderDetail engine={mine} store={withInstance} onBack={() => undefined} onEdit={() => undefined} />);
  expect(screen.getByRole("button", { name: "Remove OpenCode work" })).toBeVisible();
});

it("puts only the service's OpenCode compatibility warning above its facts", () => {
  installMockFetch();
  const message = "OpenCode 1.13 runs with limited support: some of what Orbyn offers is left out.";
  const opencode = { ...engine, id: "opencode", name: "OpenCode", tier: "acp", version: "1.13.2", support: "below_minimum",
    compatibility: { status: "unsupported", message } } as EngineStatus;
  render(<ProviderDetail engine={opencode} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  const warning = screen.getByText(message);
  expect(warning.compareDocumentPosition(screen.getByText("Version")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(screen.getAllByText(message)).toHaveLength(1);
});

it("does not infer OpenCode support from its version alone", () => {
  installMockFetch();
  const opencode = { ...engine, id: "opencode", name: "OpenCode", tier: "acp", version: "1.13.2", support: "unknown" } as EngineStatus;
  render(<ProviderDetail engine={opencode} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  expect(screen.getByText("1.13.2")).toBeVisible();
  expect(screen.queryByText(/runs with limited support/)).toBeNull();
});

async function seedCustomModels(mock: ReturnType<typeof installMockFetch>) {
  const read = mock.handle("POST", "/api/settings/preferences/query", { scope: {}, keys: ["custom_models"] }).body as { catalog: { version: string }; rows: { revision: string }[] };
  mock.handle("PUT", "/api/settings/preferences", { scope: {}, membership_version: read.catalog.version, if_match: { custom_models: read.rows[0]?.revision },
    edits: { custom_models: { op: "set", value: [{ engine: "claude", id: "wide", label: "sonnet-wide" }, { engine: "claude", id: "fast", label: "haiku-fast" }] } } });
}

it("gives only custom models arrows, disabled at the ends, and a Remove that is always there", async () => {
  const mock = installMockFetch();
  await seedCustomModels(mock);
  const user = userEvent.setup();
  render(<ProviderDetail engine={engine} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  await screen.findByRole("switch", { name: "Show sonnet-wide" });
  expect(screen.queryByRole("button", { name: "Move Claude Sonnet up" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Remove Claude Sonnet" })).toBeNull();
  expect(screen.getByRole("button", { name: "Move sonnet-wide up" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Move sonnet-wide down" })).toBeEnabled();
  expect(screen.getByRole("button", { name: "Move haiku-fast down" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Remove sonnet-wide" })).toBeVisible();
  const rows = () => screen.getAllByRole("switch").map((item) => item.getAttribute("aria-label"));
  expect(rows().slice(-2)).toEqual(["Show sonnet-wide", "Show haiku-fast"]);
  await user.click(screen.getByRole("button", { name: "Move sonnet-wide down" }));
  await waitFor(() => expect(rows().slice(-2)).toEqual(["Show haiku-fast", "Show sonnet-wide"]));
  await user.click(screen.getByRole("button", { name: "Remove haiku-fast" }));
  await waitFor(() => expect(screen.queryByRole("switch", { name: "Show haiku-fast" })).toBeNull());
  expect(within(document.body).getByRole("switch", { name: "Show sonnet-wide" })).toBeVisible();
});

it("says which binary couldn't run, in the service's own words, with Check again", async () => {
  installMockFetch();
  render(<ProviderDetail engine={{ ...engine, problem: "This engine did not respond." }} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  const banner = (await screen.findByText("Couldn’t run /opt/claude. This engine did not respond.")).closest<HTMLElement>("[role=alert]")!;
  expect(within(banner).getByRole("button", { name: "Check again" })).toBeVisible();
});

it("says which thread runs on the engine and offers it, instead of a Remove that would fail", async () => {
  installMockFetch();
  useWorkspace.setState({ threads: { t1: { id: "t1", title: "Fix login redirect", engine: "claude", active_run: { id: "r1" } } as never } });
  const removable = { ...engine, tier: "acp" } as EngineStatus;
  render(<ProviderDetail engine={removable} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  expect(await screen.findByText("A thread is running on Claude Code: “Fix login redirect”. Stop it, then remove it.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Remove Claude Code" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Open thread" })).toBeVisible();
  useWorkspace.setState({ threads: {} });
});

it("asks once, in the design's words, before removing an engine", async () => {
  installMockFetch();
  useWorkspace.setState({ threads: {} });
  const user = userEvent.setup();
  render(<ProviderDetail engine={{ ...engine, tier: "acp" } as EngineStatus} store={store} onBack={() => undefined} onEdit={() => undefined} />);
  expect(await screen.findByText("Removes this engine from Orbyn. Its sign-in and sessions stay with the engine.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Remove Claude Code" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove Claude Code?" });
  expect(within(dialog).getByText("It leaves Orbyn’s list. Its sign-in and sessions stay with the engine.")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Remove" })).toBeVisible();
});
