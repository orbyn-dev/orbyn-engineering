import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import type { EngineStatus, Project } from "../../api/types";
import { installMockFetch } from "../../test-support";
import { wireWrites } from "../../wire.test-support";
import { holdEach } from "../../wire.held.test-support";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { DRAFT_KEY } from "../../product/state/draftStore";
import { DraftView } from "./DraftView";
import { createReposMock } from "../../../mock/repos";
import { setReposApi } from "../repos";

let mock: MockApi;
/** Saves the project's default engine and model the way Settings does, and answers what the draft reads. */
const saveDefaults = (model: string, engine = "codex") =>
  mock.handle("PUT", "/api/projects/p-app/preferences", { values: { default_engine: engine, default_model: model, disabled_engines: [] } });
const created = () => wireWrites().filter((call) => call.method === "POST" && call.path === "/api/threads");
const runs = () => wireWrites().filter((call) => /^\/api\/threads\/[^/]+\/runs$/.test(call.path));
beforeEach(() => {
  mock = installMockFetch();
  const model = (id: string, label: string) => ({ id, label, description: null, is_default: false, custom: false, options: [] });
  mock.setEngineModels("codex", { engine: "codex", source: "engine", checked_at: "fixture", error: null, fallback: false, options: [], agents: [], output_styles: [],
    models: [model("remembered", "Remembered"), model("explicit", "Explicit"), model("late-default", "Late default"), model("project-model", "Project model")] });
  useWorkspace.setState({
    projects: { data: [{ id: "p-app", name: "Fixture", trusted: true, is_git: true, path: "/fixture/a" } as Project], loading: false, error: null },
    engines: { data: [{ id: "codex", name: "Codex", auth: "logged_in", installed: true, capabilities: { managed: true } } as EngineStatus], loading: false, error: null },
  });
  useUi.setState({ lastModel: { engine: "codex", model: "remembered", effort: "high" },
    drafts: { [DRAFT_KEY]: "Fixture prompt" } });
});

it("sends the project default from the real draft handler instead of the remembered model", async () => {
  saveDefaults("project-model");
  render(<DraftView projectId="p-app" />);
  const send = screen.getByRole("button", { name: "Send" });
  await waitFor(() => expect(send).toBeEnabled());
  await userEvent.setup().click(send);
  await waitFor(() => expect(created()).toHaveLength(1));
  expect(created()[0]?.body).toEqual({ project_id: "p-app", engine: "codex", model: "project-model", title: null, worktree: null, skill_profile: null });
  await waitFor(() => expect(runs()).toHaveLength(1));
  expect(runs()[0]?.body).toEqual({ prompt: "Fixture prompt" });
});

it("keeps an explicit draft model when defaults arrive later, then sends that explicit model", async () => {
  saveDefaults("late-default");
  const held = holdEach("GET", "/api/preferences?project_id=p-app");
  render(<DraftView projectId="p-app" />);
  const user = userEvent.setup();
  expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Remembered" }));
  await user.click(within(await screen.findByRole("dialog", { name: "Choose a model" })).getByRole("option", { name: /Explicit/ }));
  await act(async () => held[0]!.release());
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" })).toBeEnabled());
  expect(screen.getByRole("button", { name: "Explicit" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(created()).toHaveLength(1));
  expect(created()[0]?.body).toMatchObject({ model: "explicit" });
});

it("keeps a missing configured provider blocked until the person explicitly chooses a replacement", async () => {
  saveDefaults("missing-model", "removed-provider");
  render(<DraftView projectId="p-app" />);
  expect(await screen.findByRole("alert")).toHaveTextContent("selected default provider is unavailable");
  expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
  expect(created()).toHaveLength(0);
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Remembered" }));
  await user.click(within(await screen.findByRole("dialog", { name: "Choose a model" })).getByRole("option", { name: /Explicit/ }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Send" }));
  await waitFor(() => expect(created()).toHaveLength(1));
  expect(created()[0]?.body).toMatchObject({ engine: "codex", model: "explicit" });
});

it("shows no side panel and no launcher on a draft, even when the panel preference is open", () => {
  saveDefaults("project-model");
  useUi.setState({ panelOpen: true });
  render(<DraftView projectId="p-app" />);
  expect(screen.queryByRole("complementary", { name: "Thread context" })).toBeNull();
  expect(screen.queryByText("Open a surface")).toBeNull();
  expect(screen.queryByRole("button", { name: "Surfaces" })).toBeNull();
});

it("offers the repository dialog on the new-thread screen only when the service reports several repositories for the project", async () => {
  saveDefaults("project-model");
  // A project of one repository: the plain dev mock answers p-app with several (mock/multiRepo.ts), so the plain case says one.
  const one = { id: "p-app", name: "Fixture", path: "/fixture/app", is_git: true, trusted: true, branch: "main", branches: ["main"], has_remote: false, primary: true, clone: null };
  setReposApi(createReposMock({ repositories: [one] }));
  const first = render(<DraftView projectId="p-app" />);
  await waitFor(() => expect(screen.getByRole("button", { name: "Send" })).toBeEnabled());
  expect(screen.queryByRole("button", { name: /^Repositories/ })).toBeNull();
  first.unmount();
  setReposApi(createReposMock());
  try {
    render(<DraftView projectId="p-app" />);
    await userEvent.setup().click(await screen.findByRole("button", { name: "Repositories: 4" }));
    const dialog = await screen.findByRole("dialog", { name: /New thread in Fixture/ });
    expect(within(dialog).getByText("Engine")).toBeVisible();
    expect(within(dialog).getByText("Permission")).toBeVisible();
  } finally {
    setReposApi(null);
  }
});
