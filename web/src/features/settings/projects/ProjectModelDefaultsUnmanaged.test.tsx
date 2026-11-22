import { render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { request } from "../../../api/client";
import type { EngineStatus, PreferenceView } from "../../../api/types";
import type { ProjectScopesQuery } from "../../../generated/ProjectScopesQuery";
import { scopedFixture } from "../rows/scopedPreferenceFixture.test-support";
import { ProjectModelDefaults } from "./ProjectModelDefaults";

let engines: EngineStatus[] = [];

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));
vi.mock("../../../product/state/workspace", () => ({
  useWorkspace: (select: (state: { engines: { data: EngineStatus[] } }) => unknown) => select({ engines: { data: engines } }),
}));
vi.mock("../../../product/state/modelStore", async (original) => {
  const actual = await original<typeof import("../../../product/state/modelStore")>();
  return {
    ...actual,
    useEngineModels: () => null,
    useModelStore: (select: (state: { byEngine: Record<string, never> }) => unknown) => select({ byEngine: {} }),
  };
});

const checkout: ProjectScopesQuery = { selection: { kind: "checkout", group_key: "group-a", project_id: "a" } };
const views = (engine: string): PreferenceView[] => [
  { key: "default_engine", value: engine, default: "", source: "project", project_scoped: true },
  { key: "default_model", value: "kiro-default", default: "", source: "project", project_scoped: true },
  { key: "default_effort", value: "", default: "", source: "project", project_scoped: true },
  { key: "default_model_options", value: {}, default: {}, source: "project", project_scoped: true },
];
const engine = (capabilities: Record<string, unknown>): EngineStatus =>
  ({ id: "kiro", name: "Kiro", tier: "acp", installed: true, capabilities }) as unknown as EngineStatus;

beforeEach(() => {
  vi.mocked(request).mockReset();
  vi.mocked(request).mockResolvedValue(scopedFixture(views("kiro"), undefined, "a") as never);
});

it("shows the visible Unmanaged word on the project default model picker for an engine Orbyn cannot ask", async () => {
  engines = [engine({ managed: false })];
  render(<ProjectModelDefaults query={checkout} />);
  const trigger = await screen.findByRole("button", { name: "kiro-default" });
  expect(within(trigger).getByText("Unmanaged")).toBeVisible();
});

it("draws no visible Unmanaged word for a managed engine", async () => {
  engines = [engine({ managed: true })];
  render(<ProjectModelDefaults query={checkout} />);
  const trigger = await screen.findByRole("button", { name: "kiro-default" });
  expect(within(trigger).queryByText("Unmanaged")).toBeNull();
});

it("draws no visible Unmanaged word when the service omits the flag", async () => {
  engines = [engine({})];
  render(<ProjectModelDefaults query={checkout} />);
  const trigger = await screen.findByRole("button", { name: "kiro-default" });
  expect(within(trigger).queryByText("Unmanaged")).toBeNull();
});
