import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../mock/api";
import type { EngineStatus } from "../../api/types";
import { installMockFetch } from "../../test-support";
import { CodeModeSetting } from "./CodeModeSetting";

const engine = (patch: Partial<EngineStatus> = {}): EngineStatus =>
  ({ id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318", auth: "logged_in", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null, ...patch }) as EngineStatus;

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});
afterEach(() => vi.restoreAllMocks());

const puts = () => vi.mocked(fetch).mock.calls.filter(([, init]) => (init as RequestInit | undefined)?.method === "PUT");

describe("the Code Mode switch of an engine", () => {
  it("is off by default, says so, and names the Planned state", async () => {
    render(<CodeModeSetting engine={engine()} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    expect(screen.getByText("Planned")).toBeInTheDocument();
    expect(screen.getByText("Off by default. The three tools are not listed to this engine.")).toBeInTheDocument();
    expect(screen.getByText(/Every call in the script still asks as usual\./)).toBeInTheDocument();
    expect(puts()).toHaveLength(0);
    expect(mock.codeMode.writes).toHaveLength(0);
  });

  it("writes once per click, and turning it on names the three tools", async () => {
    render(<CodeModeSetting engine={engine()} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).toBeChecked());
    expect(mock.codeMode.writes).toEqual([{ engine: "claude", enabled: true }]);
    expect(screen.getByText("orbyn_code_search")).toBeInTheDocument();
    expect(screen.getByText("orbyn_code_describe")).toBeInTheDocument();
    expect(screen.getByText("orbyn_code_run")).toBeInTheDocument();
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).not.toBeChecked());
    expect(mock.codeMode.writes).toEqual([{ engine: "claude", enabled: true }, { engine: "claude", enabled: false }]);
  });

  it("never writes without a click, and reads the stored value when the page opens", async () => {
    mock.codeMode.handle("PUT", "/api/engines/claude/code-mode", { enabled: true });
    const before = mock.codeMode.writes.length;
    render(<CodeModeSetting engine={engine()} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeChecked());
    expect(mock.codeMode.writes).toHaveLength(before);
  });

  it("keeps each engine's own value", async () => {
    mock.codeMode.handle("PUT", "/api/engines/codex/code-mode", { enabled: true });
    const { rerender } = render(<CodeModeSetting engine={engine()} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).not.toBeChecked();
    rerender(<CodeModeSetting engine={engine({ id: "codex", name: "Codex" })} />);
    await waitFor(() => expect(screen.getByRole("switch", { name: "Use Code Mode" })).toBeChecked());
  });

  it("freezes the switch while a write is out, so a second press is not a second write", async () => {
    let finish!: () => void;
    vi.mocked(fetch).mockImplementation(async (_input, init) => {
      if ((init as RequestInit | undefined)?.method === "PUT") {
        await new Promise<void>((resolve) => (finish = resolve));
        return new Response(JSON.stringify({ enabled: true }), { status: 200 });
      }
      return new Response(JSON.stringify({ enabled: false }), { status: 200 });
    });
    render(<CodeModeSetting engine={engine()} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeEnabled());
    await userEvent.click(toggle);
    await waitFor(() => expect(toggle).toBeDisabled());
    await userEvent.click(toggle);
    expect(puts()).toHaveLength(1);
    finish();
    await waitFor(() => expect(toggle).toBeEnabled());
    expect(toggle).toBeChecked();
  });

  it("says when it could not read the setting and shows it off", async () => {
    mock.fail("/api/engines", 1, 500);
    render(<CodeModeSetting engine={engine()} />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't read this setting.");
    expect(screen.getByRole("switch", { name: "Use Code Mode" })).not.toBeChecked();
  });

  it("says when the write failed and keeps the switch where it was", async () => {
    render(<CodeModeSetting engine={engine()} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeEnabled());
    mock.fail("/api/engines", 1, 500);
    await userEvent.click(toggle);
    expect(await screen.findByRole("alert")).toHaveTextContent("Try again.");
    expect(toggle).not.toBeChecked();
    expect(toggle).toBeEnabled();
    expect(mock.codeMode.writes).toHaveLength(0);
  });

  it("adds Codex's own code mode to the description for a Codex engine", async () => {
    render(<CodeModeSetting engine={engine({ id: "codex", name: "Codex" })} />);
    expect(await screen.findByText(/Codex also has its own code mode\. Both work, and neither needs the other\./)).toBeInTheDocument();
  });
});
