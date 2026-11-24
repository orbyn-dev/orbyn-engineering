import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { wire, wireWrites } from "../../../wire.test-support";
import { ProvidersSection } from "./Providers";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

/** What the page asked the engine routes to change. */
const engineWrites = () => wireWrites().filter((call) => call.path.startsWith("/api/engines"));

/** Opens a provider's page once the list has settled into its groups (the rows are replaced when the catalogue arrives). */
async function openProvider(name: string) {
  await screen.findByText("Not built yet");
  await userEvent.click(await screen.findByRole("button", { name }));
}

/** The service reports these engines, and the page has read them. */
async function report(engines: EngineStatus[]) {
  mock.setEngines(engines);
  await useWorkspace.getState().refreshEngines();
}

const rangeWarning =
  "This engine version is outside the supported range for this Orbyn release. Use 0.166.0.";
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
    version: "0.160.0",
    auth: "logged_in",
    installer: "npm",
    latest: null,
    support: "below_minimum",
    update_command: "npm install -g @openai/codex@latest",
    manual_hint: "Update Codex with the installer you used, then refresh.",
    sign_in_command: null,
    updating: false,
    problem: null,
    ...overrides,
  };
}

describe("provider compatibility warning", () => {
  it("offers the recommended version through the installer that owns the binary", async () => {
    const user = userEvent.setup();
    await report([engine({
      compatibility: {
        status: "unsupported",
        message: rangeWarning,
        recommended_version: "0.166.0",
        recommended_command: "npm install -g @openai/codex@0.166.0",
      },
    })]);
    render(<ProvidersSection />);
    await openProvider("Codex");

    expect(await screen.findByText(rangeWarning)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Update" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Install 0.166.0" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("npm install -g @openai/codex@0.166.0")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Install 0.166.0" }));
    await waitFor(() => expect(engineWrites()).toEqual([{ method: "POST", path: "/api/engines/codex/install-recommended" }]));
    // The page reads the engines again, and the warning is gone once the service reports the version it installed.
    await waitFor(() => expect(screen.queryByText(rangeWarning)).toBeNull());
    const calls = wire();
    expect(calls.slice(calls.findIndex((call) => call.path.endsWith("/install-recommended")) + 1)).toContainEqual({ method: "GET", path: "/api/engines" });
    expect(engineWrites().some((call) => call.path.endsWith("/update"))).toBe(false);
  });

  it("keeps manual instructions when no installer can pin the version", async () => {
    await report([engine({
      compatibility: { status: "unsupported", message: rangeWarning, recommended_version: "0.166.0" },
    })]);
    render(<ProvidersSection />);
    await openProvider("Codex");

    expect(await screen.findByText(rangeWarning)).toBeVisible();
    expect(screen.queryByRole("button", { name: "Install 0.166.0" })).toBeNull();
    expect(
      screen.getByText("Install 0.166.0 with the installer that owns this engine. Update Codex with the installer you used, then refresh."),
    ).toBeVisible();
    // The installer it did prove is still offered.
    expect(screen.getByRole("button", { name: "Update" })).toBeVisible();
  });

  it("says nothing about a version the signed manifest does not flag", async () => {
    await report([engine({ version: "0.166.0", support: "tested", latest: "0.170.0" })]);
    render(<ProvidersSection />);
    await openProvider("Codex");

    expect(await screen.findByRole("button", { name: "Update to 0.170.0" })).toBeVisible();
    expect(screen.queryByText(rangeWarning)).toBeNull();
  });
});
