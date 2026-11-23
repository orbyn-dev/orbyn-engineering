import { render, screen, within } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus } from "../../../api/types";
import type { ProjectScopesQuery } from "../../../generated/ProjectScopesQuery";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { seedPreference } from "../rows/seedPreference.test-support";
import { ProjectModelDefaults } from "./ProjectModelDefaults";

let mock: MockApi;
const checkout: ProjectScopesQuery = { selection: { kind: "checkout", group_key: "mock:p-app", project_id: "p-app" } };
const engine = (capabilities: Record<string, unknown>): EngineStatus =>
  ({ id: "kiro", name: "Kiro", tier: "acp", installed: true, capabilities }) as unknown as EngineStatus;

/** The service reports Kiro with these capabilities, and the person's default is Kiro's own model. */
async function report(capabilities: Record<string, unknown>) {
  mock.setEngines([engine(capabilities)]);
  await useWorkspace.getState().refreshEngines();
  seedPreference(mock, "default_engine", "kiro");
  seedPreference(mock, "default_model", "kiro-default");
}

beforeEach(() => {
  mock = installMockFetch();
});

it("shows the visible Unmanaged word on the project default model picker for an engine Orbyn cannot ask", async () => {
  await report({ managed: false });
  render(<ProjectModelDefaults query={checkout} />);
  const trigger = await screen.findByRole("button", { name: "kiro-default" });
  expect(within(trigger).getByText("Unmanaged")).toBeVisible();
});

it("draws no visible Unmanaged word for a managed engine", async () => {
  await report({ managed: true });
  render(<ProjectModelDefaults query={checkout} />);
  const trigger = await screen.findByRole("button", { name: "kiro-default" });
  expect(within(trigger).queryByText("Unmanaged")).toBeNull();
});

it("draws no visible Unmanaged word when the service omits the flag", async () => {
  await report({});
  render(<ProjectModelDefaults query={checkout} />);
  const trigger = await screen.findByRole("button", { name: "kiro-default" });
  expect(within(trigger).queryByText("Unmanaged")).toBeNull();
});
