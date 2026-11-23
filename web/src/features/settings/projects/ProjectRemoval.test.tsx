import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { Project } from "../../../api/types";
import type { ProjectScopeCheckout } from "../../../generated/ProjectScopeCheckout";
import type { ProjectScopeGroup } from "../../../generated/ProjectScopeGroup";
import { installMockFetch } from "../../../test-support";
import { wireWrites } from "../../../wire.test-support";
import { ProjectRemoval } from "./Projects";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
});

const memberOf = (project: Project): ProjectScopeCheckout => ({
  project_id: project.id, physical_key: `k-${project.id}`, name: project.name, path: project.path,
  repository_key: null, repository_label: null, repository_relative_path: null,
  grouping_mode: "separate", metadata_complete: true, unavailable: null,
});

it("asks again in a dialog with Cancel focused and says the threads stay in Archive", async () => {
  // A project with three threads, one of them archived, and none running (the service refuses to remove one that runs).
  const project = (mock.handle("GET", "/api/projects").body as Project[]).find((entry) => entry.id === "p-new")!;
  const made = [1, 2, 3].map(() => (mock.handle("POST", "/api/threads", { engine: "claude", project_id: project.id }).body as { id: string }).id);
  mock.handle("PATCH", `/api/threads/${made[2]}`, { archived: true });
  const threads = 3;
  const member = memberOf(project);
  const group: ProjectScopeGroup = { key: "g1", label: project.name, members: [member] };
  const query = { selection: { kind: "checkout" as const, group_key: "g1", project_id: project.id } };
  const user = userEvent.setup();
  render(<ProjectRemoval query={query} group={group} members={[member]} disabled={false} reload={() => {}} clearSelection={() => {}} />);
  expect(screen.getByText("Stops tracking it in Orbyn. Files on disk stay.")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Remove…" }));
  const dialog = await screen.findByRole("dialog", { name: `Remove ${project.name}?` });
  expect(within(dialog).getByText("Orbyn stops tracking this project. Files on disk are not touched.")).toBeVisible();
  expect(within(dialog).getByText(project.path)).toBeVisible();
  expect(within(dialog).getByText(`${threads} · stay in Archive`)).toBeVisible();
  await waitFor(() => expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(wireWrites()).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Remove project" }));
  await waitFor(() => expect(wireWrites().filter((call) => call.method === "DELETE")).toEqual([{ method: "DELETE", path: `/api/projects/${project.id}` }]));
  expect(mock.handle("GET", "/api/projects").body).not.toContainEqual(expect.objectContaining({ id: project.id }));
});
