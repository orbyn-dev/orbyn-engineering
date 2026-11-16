import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { EngineStatus } from "../../../api/types";
import type { EngineInstalls } from "../../../generated/EngineInstalls";
import type { InstallOutcome } from "../../../generated/InstallOutcome";
import { installMockFetch } from "../../../test-support";
import { useWorkspace } from "../../../product/state/workspace";
import { useEngineUpdates } from "./engineUpdates";
import { ProviderDetail } from "./ProviderDetail";

const codex = { id: "codex", name: "Codex", tier: "native", installed: true, version: "0.166.0", latest: "0.167.0", auth: "logged_in", path: "/usr/local/bin/codex",
  capabilities: {}, support: "tested", update_command: "npm install -g @openai/codex@latest", sign_in_command: null, updating: false, problem: null } as unknown as EngineStatus;
const store = { instances: [], raw: [], disabled: false, save: async () => null } as never;
const COMMAND = "npm install -g @openai/codex@latest";

function outcome(patch: Partial<InstallOutcome> = {}): InstallOutcome {
  const finished = new Date().toISOString();
  return { id: "o1", engine: "codex", kind: "update", started_at: new Date(Date.now() - 4000).toISOString(), finished_at: finished, ok: true,
    command: COMMAND, exit_status: 0, from_version: "0.166.0", to_version: "0.167.0", output: ["added 1 package", "changed 2 packages in 4s"], ...patch };
}

/** The mock service, with what it keeps of installs replaced by what a test says. */
function serve(answer: Partial<EngineInstalls>) {
  const mock = installMockFetch();
  const base = globalThis.fetch;
  const fetch = vi.fn(async (input: string, init?: RequestInit) =>
    input.startsWith("/api/engines/") && input.endsWith("/installs")
      ? new Response(JSON.stringify({ engine: "codex", outcomes: [], ...answer }), { status: 200, headers: { "Content-Type": "application/json" } })
      : base(input, init));
  vi.stubGlobal("fetch", fetch);
  return { mock, fetch };
}

const open = (engine: EngineStatus = codex) => render(<ProviderDetail engine={engine} store={store} onBack={() => undefined} onEdit={() => undefined} />);

beforeEach(() => { useWorkspace.setState({ threads: {} }); useEngineUpdates.setState({ running: {}, said: {}, logs: {}, ended: {} }); });
afterEach(() => { vi.unstubAllGlobals(); });

it("shows the running installer's command and lines, says it cannot be stopped and offers no cancel", async () => {
  serve({});
  open({ ...codex, updating: true });
  act(() => {
    useEngineUpdates.getState().apply({ kind: "started", engine: "codex", command: COMMAND });
    useEngineUpdates.getState().apply({ kind: "output", engine: "codex", line: "added 1 package" });
  });
  expect(await screen.findByText("0.166.0 · updating to 0.167.0")).toBeVisible();
  expect(screen.getByText("Updating…")).toBeVisible();
  const log = screen.getByRole("log", { name: "Installer output" });
  expect(within(log).getByText(COMMAND)).toBeVisible();
  expect(within(log).getByText("added 1 package")).toBeVisible();
  expect(screen.getByText(/An update can’t be stopped once it starts/)).toBeVisible();
  expect(screen.queryByRole("button", { name: /cancel|stop/i })).toBeNull();
});

it("says the new version and how long it took, with the installer's lines, when the update finished", async () => {
  serve({ outcomes: [outcome()] });
  open({ ...codex, version: "0.167.0", latest: "0.167.0" });
  expect(await screen.findByText("Updated to 0.167.0 in 4 s.")).toBeVisible();
  const log = screen.getByRole("log", { name: "Installer output" });
  expect(within(log).getByText("changed 2 packages in 4s")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Try again" })).toBeNull();
});

it("does not repeat an old success", async () => {
  serve({ outcomes: [outcome({ finished_at: new Date(Date.now() - 3 * 3600_000).toISOString() })] });
  open({ ...codex, version: "0.167.0", latest: "0.167.0" });
  await screen.findByText("Version");
  await waitFor(() => expect(screen.queryByText(/Updated to/)).toBeNull());
  expect(screen.queryByRole("log")).toBeNull();
});

it("names the failure and the old version, marks the installer's last line, and offers Try again and Copy details", async () => {
  const user = userEvent.setup();
  const copied = vi.fn(async () => undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText: copied }, configurable: true });
  const { fetch } = serve({ outcomes: [outcome({ ok: false, to_version: undefined, exit_status: 243,
    reason: "The installer stopped with an error (exit status 243).", output: ["npm error EACCES: permission denied, mkdir ‘/usr/local/lib/node_modules’"] })] });
  open();
  const banner = await screen.findByRole("alert");
  expect(banner).toHaveTextContent("Couldn’t update Codex. It is still on 0.166.0.");
  expect(banner).toHaveTextContent("The installer stopped with an error (exit status 243).");
  expect(within(screen.getByRole("log")).getByText(/EACCES/)).toHaveClass("bad");

  await user.click(within(banner).getByRole("button", { name: "Copy details" }));
  expect(copied).toHaveBeenCalledWith(expect.stringContaining("exit status 243"));
  expect(copied).toHaveBeenCalledWith(expect.stringContaining(`$ ${COMMAND}`));

  await user.click(within(banner).getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(fetch).toHaveBeenCalledWith("/api/engines/codex/update", expect.objectContaining({ method: "POST" })));
});

it("sends an engine that is not installed to the vendor's own page and installs nothing", async () => {
  const user = userEvent.setup();
  const { fetch } = serve({ vendor_page: { url: "https://cursor.com/cli", domain: "cursor.com" } });
  open({ ...codex, id: "cursor", name: "Cursor", installed: false, version: null, latest: null, path: null, update_command: null });
  expect(await screen.findByText("Cursor isn’t installed on this computer.")).toBeVisible();
  const link = screen.getByRole("link", { name: /Open Cursor’s instructions · cursor.com/ });
  expect(link).toHaveAttribute("href", "https://cursor.com/cli");
  expect(link).toHaveAttribute("rel", expect.stringContaining("noopener"));
  expect(screen.getByText(/Orbyn installs nothing itself/)).toBeVisible();
  expect(screen.queryByRole("button", { name: /^Install/ })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Check again" }));
  await waitFor(() => expect(fetch).toHaveBeenCalledWith("/api/engines/refresh", expect.objectContaining({ method: "POST" })));
});

it("tries a failed registry install again through the registry", async () => {
  const user = userEvent.setup();
  const { fetch } = serve({ outcomes: [outcome({ engine: "acp-pal", kind: "install", ok: false, from_version: undefined, reason: "That agent's archive could not be downloaded.", output: [] })] });
  open({ ...codex, id: "acp-pal", name: "Pal Agent", tier: "acp", update_command: null });
  await user.click(await screen.findByRole("button", { name: "Try again" }));
  await waitFor(() => expect(fetch).toHaveBeenCalledWith("/api/acp/registry/pal", expect.objectContaining({ method: "POST" })));
  expect(fetch).not.toHaveBeenCalledWith(expect.stringContaining("install-recommended"), expect.anything());
});
