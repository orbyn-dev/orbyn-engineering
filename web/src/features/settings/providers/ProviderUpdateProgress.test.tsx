import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wireWrites } from "../../../wire.test-support";
import { useEngineUpdates } from "./engineUpdates";
import { ProvidersSection } from "./Providers";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  mock.reset("empty");
  useEngineUpdates.getState().clear();
});

/** The service reports these engines, and the page has read them. */
async function report(engines: EngineStatus[]) {
  mock.setEngines(engines);
  await useWorkspace.getState().refreshEngines();
}

/** Opens a provider's page once the list has settled into its groups (the rows are replaced when the catalogue arrives). */
async function openProvider(name: string) {
  await screen.findByText("Not built yet");
  await userEvent.click(await screen.findByRole("button", { name }));
}

function engine(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return {
    id: "codex",
    name: "Codex",
    tier: "native",
    capabilities: {
      managed: true, streaming: true, approvals: true, resume: true, mcp: true,
      sandbox: true, skill_delivery: [], goal_control: true, decline_feedback: false,
    permission_modes: ["supervised", "auto_edits", "auto", "full_access"],
    },
    installed: true,
    path: "/engine",
    version: "0.166.0",
    auth: "logged_in",
    installer: "npm",
    latest: "0.167.0",
    support: "tested",
    update_command: "npm install -g @openai/codex@latest",
    manual_hint: "Update Codex with the installer you used, then refresh.",
    sign_in_command: null,
    updating: false,
    problem: null,
    ...overrides,
  };
}

/** The service's update stream, sending what the test writes to it; every other request goes to the mock service. */
function stream() {
  const service = fetch;
  let send: (text: string) => void = () => {};
  const body = new ReadableStream<Uint8Array>({
    start(controller) {
      const encoder = new TextEncoder();
      send = (text) => controller.enqueue(encoder.encode(text));
    },
  });
  const fetchStub = vi.fn(async (url: string, init?: RequestInit) =>
    url.endsWith("/api/engines/update-events")
      ? new Response(body, { headers: { "Content-Type": "text/event-stream" } })
      : service(url, init),
  );
  vi.stubGlobal("fetch", fetchStub);
  return { fetch: fetchStub, event: (value: object) => act(() => send(`data: ${JSON.stringify(value)}\n\n`)) };
}

describe("provider update progress", () => {
  it("shows the installer's command and lines while the update runs, and drops them when it finished", async () => {
    await report([engine()]);
    const updates = stream();
    render(<ProvidersSection />);
    await openProvider("Codex");
    await waitFor(() => expect(updates.fetch).toHaveBeenCalledWith("/api/engines/update-events", expect.anything()));
    expect(await screen.findByRole("button", { name: "Update to 0.167.0" })).toBeVisible();

    await updates.event({ kind: "started", engine: "codex", command: "npm install -g @openai/codex@latest" });
    const log = await screen.findByRole("log", { name: "Installer output" });
    expect(within(log).getByText("npm install -g @openai/codex@latest")).toBeVisible();
    expect(screen.getByText("Updating…")).toBeVisible();
    await updates.event({ kind: "output", engine: "codex", line: "added 1 package in 3s" });
    expect(await within(log).findByText("added 1 package in 3s")).toBeVisible();
    // The command stays above the lines.
    expect(within(log).getByText("npm install -g @openai/codex@latest")).toBeVisible();

    await updates.event({ kind: "finished", engine: "codex", version: "0.167.0", error: null });
    await waitFor(() => expect(screen.queryByText("Updating…")).toBeNull());
  });

  it("keeps the update dialog open with the line, then its error", async () => {
    const user = userEvent.setup();
    await report([engine()]);
    const updates = stream();
    // The installer is still running when the lines arrive; the service then reports that it failed.
    const slow = holdFetch("POST", "/api/engines/codex/update");
    mock.fail("/api/engines/codex/update", 1, 502, "update_failed", "The update failed.");
    render(<ProvidersSection />);
    await openProvider("Codex");
    await waitFor(() => expect(updates.fetch).toHaveBeenCalledWith("/api/engines/update-events", expect.anything()));
    await user.click(await screen.findByRole("button", { name: "Update to 0.167.0" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Update" }));
    await waitFor(() => expect(wireWrites()).toContainEqual({ method: "POST", path: "/api/engines/codex/update" }));

    await updates.event({ kind: "started", engine: "codex", command: "npm install -g @openai/codex@latest" });
    await updates.event({ kind: "output", engine: "codex", line: "npm error code EACCES" });
    expect(await within(dialog).findByText("npm error code EACCES")).toBeVisible();
    await updates.event({ kind: "finished", engine: "codex", version: null, error: "The update failed." });
    slow.release();
    expect(await within(screen.getByRole("dialog")).findByRole("alert")).toHaveTextContent("The update failed.");
  });

  it("ignores another engine's update", async () => {
    await report([engine()]);
    const updates = stream();
    render(<ProvidersSection />);
    await openProvider("Codex");
    await waitFor(() => expect(updates.fetch).toHaveBeenCalledWith("/api/engines/update-events", expect.anything()));
    await updates.event({ kind: "output", engine: "claude", line: "changed 3 packages" });
    expect(screen.queryByText("changed 3 packages")).toBeNull();
    expect(screen.getByRole("button", { name: "Update to 0.167.0" })).toBeVisible();
  });
});
