import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import type { EngineStatus } from "../../../api/types";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import { createEngineHostMock } from "../../../../mock/engineHost";
import { setEngineHostApi } from "../../enginehost";
import { ProviderTable } from "./ProviderTable";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: vi.fn(), reload: vi.fn() };
const claude = { id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318",
  auth: "logged_in", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null } as EngineStatus;
const entry: ProviderEntry = { id: "claude", name: "Claude Code", group: "in_use", labels: [], registered: true, openable: true };

function show(open = vi.fn(), engine: EngineStatus = claude, provider: ProviderEntry = entry) {
  vi.mocked(request).mockResolvedValue({ catalog: { version: "1", selection: { kind: "all_projects" } }, rows: [] } as never);
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
  // The path is the setting, not a row chip: neither row labels itself as selected.
  expect(screen.queryByText("Selected")).toBeNull();
});

it("reads the one engine's signed-out state on both rows", async () => {
  show(vi.fn(), { ...claude, auth: "logged_out", account: undefined }, { ...entry, labels: ["signed_out"] });
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
