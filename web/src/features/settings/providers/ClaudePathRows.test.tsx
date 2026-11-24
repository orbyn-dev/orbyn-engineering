import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { setEngineHostApi } from "../../enginehost";
import { installMockFetch } from "../../../test-support";
import { wireWrites } from "../../../wire.test-support";
import { seedPreference } from "../rows/seedPreference.test-support";
import { ProviderTable } from "./ProviderTable";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: vi.fn(), reload: vi.fn() };
const claude = { id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318",
  auth: "logged_in", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null } as EngineStatus;
const entry: ProviderEntry = { id: "claude", name: "Claude Code", group: "in_use", labels: [], registered: true, openable: true };

type Path = "sdk" | "cli";

/** The In use writes the table sent, so a test can see that navigation alone writes nothing. */
const putBodies = () => wireWrites()
  .filter((call) => call.method === "PUT" && call.path === "/api/settings/preferences")
  .map((call) => call.body);

function show(path: Path = "sdk", open = vi.fn(), engine: EngineStatus = claude, provider: ProviderEntry = entry) {
  seedPreference(mock, "claude_engine_path", path);
  seedPreference(mock, "disabled_engines", []);
  render(<ProviderTable store={store} onOpen={open} onEdit={() => undefined} engines={[engine]} catalog={{ providers: [provider] }} />);
  return open;
}

const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;

afterEach(() => setEngineHostApi(null));

it("draws Claude (SDK) and Claude (CLI), each with the engine's version and state and its own helper line", async () => {
  setEngineHostApi(createEngineHostMock({ hosts: [{ id: "claude", name: "Claude Code", state: "running", can_retry: false }] }));
  show();
  const sdk = rowOf("Claude (SDK)");
  const cli = rowOf("Claude (CLI)");
  for (const row of [sdk, cli]) {
    expect(within(row).getByText("2.1.318")).toBeVisible();
    expect(within(row).getByText("Ready")).toBeVisible();
  }
  expect(within(sdk).getByText(/Runs through Orbyn’s engine host with the Claude Agent SDK, using your own Claude Code\. Default\./)).toBeVisible();
  expect(within(cli).getByText("Runs your Claude Code directly, without the engine host.")).toBeVisible();
  // The SDK row carries the engine host's own state; the CLI row does not.
  expect(await within(sdk).findByText("Running")).toBeVisible();
  expect(within(cli).queryByText("Running")).toBeNull();
  // The chosen path alone carries the setting's switch; the other row says it is not the choice.
  expect(await within(sdk).findByRole("switch", { name: "Use Claude (SDK)" })).toBeVisible();
  expect(within(cli).queryByRole("switch")).toBeNull();
  expect(within(cli).getByText("Not selected")).toBeVisible();
});

it("reads the one engine's signed-out state on both rows", async () => {
  show("sdk", vi.fn(), { ...claude, auth: "logged_out", account: undefined }, { ...entry, labels: ["signed_out"] });
  const sdk = rowOf("Claude (SDK)");
  const cli = rowOf("Claude (CLI)");
  for (const row of [sdk, cli]) {
    expect(within(row).getByText("Signed out")).toBeVisible();
    expect(within(row).queryByText("Ready")).toBeNull();
  }
});

it("opens the one Claude Code page from either row", async () => {
  const open = show();
  await screen.findByText("Claude (CLI)", { selector: ".name" });
  await userEvent.click(screen.getByRole("button", { name: "Claude (SDK)" }));
  await userEvent.click(screen.getByRole("button", { name: "Claude (CLI)" }));
  expect(open.mock.calls).toEqual([["claude"], ["claude"]]);
});

it("keeps the switch on the SDK row when the SDK is chosen, and says Not selected on the CLI row", async () => {
  show("sdk");
  const sdk = rowOf("Claude (SDK)");
  const cli = rowOf("Claude (CLI)");
  expect(await within(sdk).findByRole("switch", { name: "Use Claude (SDK)" })).toBeVisible();
  expect(within(cli).queryByRole("switch")).toBeNull();
  expect(within(cli).getByText("Not selected")).toBeVisible();
  expect(within(cli).getByRole("button", { name: "Choose in Claude path for Claude (CLI)" })).toBeVisible();
  expect(within(sdk).queryByRole("button", { name: /Choose in Claude path/ })).toBeNull();
  expect(await screen.findAllByRole("switch")).toHaveLength(1);
});

it("keeps the switch on the CLI row when the CLI is chosen, and says Not selected on the SDK row", async () => {
  show("cli");
  const sdk = rowOf("Claude (SDK)");
  const cli = rowOf("Claude (CLI)");
  expect(await within(cli).findByRole("switch", { name: "Use Claude (CLI)" })).toBeVisible();
  expect(within(sdk).queryByRole("switch")).toBeNull();
  expect(within(sdk).getByText("Not selected")).toBeVisible();
  expect(within(sdk).getByRole("button", { name: "Choose in Claude path for Claude (SDK)" })).toBeVisible();
  expect(within(cli).queryByRole("button", { name: /Choose in Claude path/ })).toBeNull();
  expect(await screen.findAllByRole("switch")).toHaveLength(1);
});

it("opens the Claude page from the other row's link without writing a preference", async () => {
  const user = userEvent.setup();
  const open = show("sdk");
  const link = await screen.findByRole("button", { name: "Choose in Claude path for Claude (CLI)" });
  await user.click(link);
  expect(open).toHaveBeenCalledExactlyOnceWith("claude");
  expect(putBodies()).toHaveLength(0);
});

it("still turns the Claude engine off from the chosen row's switch", async () => {
  const user = userEvent.setup();
  show("sdk");
  const toggle = await screen.findByRole("switch", { name: "Use Claude (SDK)" });
  await waitFor(() => expect(toggle).toBeEnabled());
  await user.click(toggle);
  await waitFor(() => expect(putBodies()).toHaveLength(1));
  expect(putBodies()[0]).toMatchObject({ edits: { disabled_engines: { op: "set", value: ["claude"] } } });
});
