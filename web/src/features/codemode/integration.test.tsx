import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { EngineStatus, EngineTools } from "../../api/types";
import { installMockFetch } from "../../test-support";
import { ProviderDetail } from "../settings/providers/ProviderDetail";
import { ToolsList } from "../tools/ToolsList";

const engine = (patch: Partial<EngineStatus> = {}): EngineStatus =>
  ({ id: "claude", name: "Claude Code", tier: "native", installed: true, path: "/bin/claude", version: "2.1.318", auth: "logged_in", latest: null, support: "tested", update_command: null, sign_in_command: null, updating: false, problem: null, ...patch }) as EngineStatus;
const store = { raw: [], instances: [], disabled: false, error: null, conflict: false, save: async () => null, reload: async () => undefined };

beforeEach(() => {
  installMockFetch();
});

describe("Settings › Providers › an engine", () => {
  it("has a Code Mode group between the engine's facts and its models, with the Planned pill", async () => {
    render(<ProviderDetail engine={engine()} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
    expect(await screen.findByRole("switch", { name: "Use Code Mode" })).toBeInTheDocument();
    const heads = [...document.querySelectorAll(".set-grp")].map((node) => node.textContent ?? "");
    const code = heads.findIndex((text) => text.startsWith("Code Mode"));
    expect(code).toBeGreaterThanOrEqual(0);
    expect(heads[code]).toContain("Planned");
    const models = heads.indexOf("Models");
    if (models >= 0) expect(code).toBeLessThan(models);
  });

  it("has no Code Mode group for an engine that is not installed", () => {
    render(<ProviderDetail engine={engine({ installed: false, path: null, version: null })} store={store as never} onBack={() => undefined} onEdit={() => undefined} />);
    expect(screen.queryByRole("switch", { name: "Use Code Mode" })).toBeNull();
  });
});

describe("the Tools tab", () => {
  const tools = (names: string[]): EngineTools =>
    ({ reported: true, error: null, incomplete: false, not_reported: [], groups: [{ id: "orbyn", count: names.length, items: names.map((name) => ({ name, state: "ready", description: "x", source: null, tools: null, reason: null, detail: null, sign_in_url: null })) }] }) as unknown as EngineTools;

  it("marks only the three Code Mode tools Planned", async () => {
    render(<ToolsList tools={tools(["orbyn_code_search", "orbyn_code_describe", "orbyn_code_run", "orbyn_thread_list"])} engineName="Claude Code" mode="thread" />);
    await userEvent.click(screen.getByRole("button", { name: /Orbyn/ }));
    expect(screen.getAllByText("Planned")).toHaveLength(3);
    const ordinary = screen.getByText("orbyn_thread_list").closest("li")!;
    expect(within(ordinary).queryByText("Planned")).toBeNull();
  });

  it("does not mark a tool of another group that happens to share a name", async () => {
    const other = { ...tools(["orbyn_code_run"]), groups: [{ id: "mcp", count: 1, items: [{ name: "orbyn_code_run", state: "ready", description: "x", source: null, tools: null, reason: null, detail: null, sign_in_url: null }] }] } as unknown as EngineTools;
    render(<ToolsList tools={other} engineName="Claude Code" mode="thread" />);
    expect(screen.queryByText("Planned")).toBeNull();
  });
});
