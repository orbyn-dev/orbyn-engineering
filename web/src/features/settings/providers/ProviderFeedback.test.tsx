import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wire, wireWrites } from "../../../wire.test-support";
import { ProvidersSection } from "./Providers";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  // No thread is running, so an engine is only busy when a test says it is.
  mock.reset("empty");
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

const engineWrites = () => wireWrites().filter((call) => call.path.startsWith("/api/engines"));

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
    latest: null,
    support: "tested",
    update_command: null,
    manual_hint: null,
    sign_in_command: null,
    updating: false,
    problem: null,
    ...overrides,
  };
}

describe("codex feedback", () => {
  it("sends the written reason and the chosen log setting", async () => {
    const user = userEvent.setup();
    await report([engine()]);
    render(<ProvidersSection />);
    await openProvider("Codex");

    await user.click(await screen.findByRole("button", { name: "Send feedback" }));
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText(/goes to OpenAI through Codex/)).toBeVisible();
    const logs = within(dialog).getByRole("switch", { name: "Include Codex logs" });
    expect(logs).not.toBeChecked();
    await user.type(within(dialog).getByLabelText("What went wrong? (optional)"), "  it broke  ");
    await user.click(logs);
    await user.click(within(dialog).getByRole("button", { name: "Send feedback" }));

    await waitFor(() =>
      expect(engineWrites()).toEqual([{
        method: "POST",
        path: "/api/engines/codex/feedback",
        body: { reason: "it broke", include_logs: true, thread_id: null },
      }]),
    );
    expect(await screen.findByRole("status")).toHaveTextContent("report-1");
  });

  it("keeps the dialog open and reports a failed send", async () => {
    const user = userEvent.setup();
    await report([engine()]);
    mock.fail("/api/engines/codex/feedback", 1, 422, "unsupported", "Codex refused the report.");
    render(<ProvidersSection />);
    await openProvider("Codex");

    await user.click(await screen.findByRole("button", { name: "Send feedback" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Send feedback" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent("Codex refused the report.");
    expect(screen.getByRole("dialog")).toBeVisible();
  });

  it("blocks duplicate sends and closing while the report is pending", async () => {
    const user = userEvent.setup();
    await report([engine()]);
    const slow = holdFetch("POST", "/api/engines/codex/feedback");
    render(<ProvidersSection />);
    await openProvider("Codex");
    await user.click(await screen.findByRole("button", { name: "Send feedback" }));
    const dialog = await screen.findByRole("dialog");
    const send = within(dialog).getByRole("button", { name: "Send feedback" });
    await user.click(send);
    expect(send).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
    await user.click(send);
    expect(engineWrites()).toHaveLength(1);
    slow.release();
    expect(await screen.findByRole("status")).toHaveTextContent("report-1");
    expect(engineWrites()).toHaveLength(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("offers the action only where the route exists", async () => {
    await report([engine({ id: "claude", name: "Claude" })]);
    render(<ProvidersSection />);
    await openProvider("Claude (SDK)");

    expect(await screen.findByRole("heading", { name: "Claude" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Send feedback" })).toBeNull();
  });
});

describe("update all", () => {
  const owned = { update_command: "npm install -g x@latest", latest: "2.0.0", version: "1.0.0" };

  it("offers update all for two owned engines with newer versions and reports a failure", async () => {
    const user = userEvent.setup();
    await report([
      engine({ ...owned, id: "codex", name: "Codex" }),
      engine({ ...owned, id: "claude", name: "Claude", update_command: "npm install -g y@latest" }),
      engine({ ...owned, id: "work", name: "Work", instance_of: "codex" }),
    ]);
    mock.fail("/api/engines/claude/update", 1, 409, "engine_busy", "claude has an active run or update");
    render(<ProvidersSection />);

    await screen.findByText("Not built yet");
    await user.click(await screen.findByRole("button", { name: "Update all" }));
    const dialog = await screen.findByRole("dialog", { name: "Update 2 engines?" });
    expect(within(dialog).getByText("npm install -g x@latest")).toBeVisible();
    expect(within(dialog).getByText("npm install -g y@latest")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Update 2 engines" }));

    expect(await screen.findByRole("heading", { name: "Updating engines" })).toBeVisible();
    expect(await screen.findByText("1 of 2 engines couldn’t be updated.")).toBeVisible();
    expect(screen.getByText("claude has an active run or update")).toBeVisible();
    expect(engineWrites()).toEqual([{ method: "POST", path: "/api/engines/update-all" }]);
    // The page reads the engines again once the update ends.
    const calls = wire();
    await waitFor(() => expect(wire().slice(calls.findIndex((call) => call.path === "/api/engines/update-all") + 1)).toContainEqual({ method: "GET", path: "/api/engines" }));
  });

  it("names an engine with no installer as skipped, and Try again runs only the one that failed", async () => {
    const user = userEvent.setup();
    await report([
      engine({ ...owned, id: "codex", name: "Codex" }),
      engine({ ...owned, id: "claude", name: "Claude", update_command: "npm install -g y@latest" }),
      engine({ ...owned, id: "gemini", name: "Gemini CLI", update_command: null }),
    ]);
    mock.fail("/api/engines/claude/update", 1, 502, "update_failed", "brew: Permission denied");
    render(<ProvidersSection />);

    await screen.findByText("Not built yet");
    await user.click(await screen.findByRole("button", { name: "Update all" }));
    const dialog = await screen.findByRole("dialog", { name: "Update 2 engines?" });
    expect(within(dialog).getByText("Skipped: Gemini CLI has no installer Orbyn recognises.")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Update 2 engines" }));

    await user.click(await screen.findByRole("button", { name: "Try again" }));
    expect(engineWrites()).toEqual([
      { method: "POST", path: "/api/engines/update-all" },
      { method: "POST", path: "/api/engines/claude/update" },
    ]);
    await waitFor(() => expect(screen.queryByRole("button", { name: "Try again" })).toBeNull());
    expect(screen.queryByText("1 of 2 engines couldn’t be updated.")).toBeNull();
  });

  it("does not offer update all for a single engine or one already current", async () => {
    await report([engine({ ...owned }), engine({ ...owned, id: "claude", name: "Claude", latest: "1.0.0" })]);
    render(<ProvidersSection />);
    expect(await screen.findByRole("button", { name: "Check again" })).toBeVisible();
    expect(screen.queryByRole("button", { name: "Update all" })).toBeNull();
  });
});

describe("acp sign-in", () => {
  it("shows the terminal sign-in an ACP agent named, and nothing when it named none", async () => {
    const acp = { id: "opencode", name: "OpenCode", tier: "acp" as const, auth: "not_applicable" as const, installer: "unknown" as const };
    await report([engine({ ...acp, sign_in_command: "opencode acp --login" })]);
    const { unmount } = render(<ProvidersSection />);
    await openProvider("OpenCode");
    expect(await screen.findByRole("button", { name: "Copy sign-in command" })).toBeVisible();
    expect(screen.getByText("opencode acp --login")).toBeVisible();
    unmount();

    await report([engine({ ...acp, sign_in_command: null })]);
    render(<ProvidersSection />);
    await openProvider("OpenCode");
    expect((await screen.findAllByText("OpenCode")).length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "Copy sign-in command" })).toBeNull();
  });
});
