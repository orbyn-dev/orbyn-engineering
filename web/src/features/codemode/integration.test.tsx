import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import type { EngineStatus, EngineTools } from "../../api/types";
import { installMockFetch } from "../../test-support";
import { ProviderDetail } from "../settings/providers/ProviderDetail";
import { ToolsList } from "../tools/ToolsList";

const engine = (patch: Partial<EngineStatus> = {}): EngineStatus =>
  ({ id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318", auth: "logged_in", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null, ...patch }) as EngineStatus;
const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: async () => null, reload: async () => undefined };

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

describe("Settings › Providers › an engine", () => {
  it("has a Code Mode group between the engine's facts and its models, without the Planned pill once the service answers", async () => {
    render(<ProviderDetail engine={engine()} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    await waitFor(() => expect(toggle).toBeEnabled());
    const heads = [...document.querySelectorAll(".set-grp")].map((node) => node.textContent ?? "");
    const code = heads.findIndex((text) => text.startsWith("Code Mode"));
    expect(code).toBeGreaterThanOrEqual(0);
    expect(heads[code]).not.toContain("Planned");
    const models = heads.indexOf("Models");
    if (models >= 0) expect(code).toBeLessThan(models);
  });

  it("keeps the Planned pill and a frozen switch when the service does not offer Code Mode (every route 404)", async () => {
    mock.codeMode.setOffered(false);
    render(<ProviderDetail engine={engine()} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
    const toggle = await screen.findByRole("switch", { name: "Use Code Mode" });
    // Real wait: asserts that nothing more happens after a pause; there is no event to wait for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    const heads = [...document.querySelectorAll(".set-grp")].map((node) => node.textContent ?? "");
    expect(heads.find((text) => text.startsWith("Code Mode"))).toContain("Planned");
    expect(toggle).toBeDisabled();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("has no Code Mode group for an engine that is not installed", () => {
    render(<ProviderDetail engine={engine({ installed: false, path: null, version: null })} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
    expect(screen.queryByRole("switch", { name: "Use Code Mode" })).toBeNull();
  });
});

describe("the Tools tab", () => {
  const tools = (names: string[]): EngineTools =>
    ({ reported: true, error: null, incomplete: false, not_reported: [], groups: [{ id: "orbyn", count: names.length, items: names.map((name) => ({ name, state: "ready", description: "x", source: null, tools: null, reason: null, detail: null, sign_in_url: null })) }] }) as unknown as EngineTools;

  it("lists the three Code Mode tools like any other, with no Planned mark", async () => {
    render(<ToolsList tools={tools(["orbyn_code_search", "orbyn_code_describe", "orbyn_code_run", "orbyn_thread_list"])} engineName="Claude Code" mode="thread" />);
    await userEvent.click(screen.getByRole("button", { name: /Orbyn/ }));
    for (const name of ["orbyn_code_search", "orbyn_code_describe", "orbyn_code_run", "orbyn_thread_list"]) {
      expect(within(screen.getByText(name).closest("li")!).queryByText("Planned")).toBeNull();
    }
    expect(screen.queryByText("Planned")).toBeNull();
  });
});
