import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { EngineStatus, JsonValue, ModelInfo } from "../../../api/types";
import type { ProjectScopesQuery } from "../../../generated/ProjectScopesQuery";
import type { ScopedPreferencesChange } from "../../../generated/ScopedPreferencesChange";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { holdFetch, wireWrites } from "../../../wire.test-support";
import { seedPreference } from "../rows/seedPreference.test-support";
import { ProjectModelDefaults } from "./ProjectModelDefaults";

let mock: MockApi;
const checkoutOf = (id: string, group = `mock:${id}`): ProjectScopesQuery => ({ selection: { kind: "checkout", group_key: group, project_id: id } });
const checkout = checkoutOf("p-app");
const together: ProjectScopesQuery = { selection: { kind: "project_group", group_key: "mock:group:p-app+p-site" } };
const keys = ["default_engine", "default_model", "default_effort", "default_model_options"];

const model = (id: string, options: ModelInfo["options"] = []): ModelInfo => ({ id, label: id, description: null, is_default: id === "old", custom: false, options });
const codex = { id: "codex", name: "Codex", tier: "native", installed: true, capabilities: { managed: true, approvals: true } } as unknown as EngineStatus;

/** What the person saved for one project: its own values, over the global ones. */
const saveFor = (project: string, values: Record<string, JsonValue>) => mock.handle("PUT", `/api/projects/${project}/preferences`, { values });
const tuple = (engine: string, model: string, effort = "", options: Record<string, string> = {}) => ({
  default_engine: { op: "set", value: engine }, default_model: { op: "set", value: model },
  default_effort: { op: "set", value: effort }, default_model_options: { op: "set", value: options } });
const every = (edit: unknown) => ({ default_engine: edit, default_model: edit, default_effort: edit, default_model_options: edit });
/** What the service holds for these keys now: the catalogue version and each row's revision, which a save must start from. */
const holds = (scope: ProjectScopesQuery) => {
  const read = mock.handle("POST", "/api/settings/preferences/query", { scope, keys }).body as ScopedPreferencesResult;
  return { version: read.catalog.version, revisions: Object.fromEntries(read.rows.map((row) => [row.key, row.revision])) };
};
const saves = () => wireWrites().filter((call) => call.method === "PUT" && call.path === "/api/settings/preferences").map((call) => call.body as ScopedPreferencesChange);
const pick = async (user: ReturnType<typeof userEvent.setup>, name: string, choice: string) => {
  await user.click(await screen.findByRole("button", { name }));
  await user.click(within(await screen.findByRole("dialog", { name: "Choose a model" })).getByRole("option", { name: new RegExp(choice) }));
};

beforeEach(async () => {
  mock = installMockFetch();
  mock.setEngines([codex]);
  mock.setEngineModels("codex", ["old", "chosen", "b-model", "old-reply"].map((id) => model(id, id === "old" ? [
    { id: "effort", label: "Effort", values: [{ id: "low", label: "Low", default: false }, { id: "high", label: "High", default: true }] },
    { id: "service_tier", label: "Speed", values: [{ id: "fast", label: "Fast", default: false }] },
  ] : [])));
  await useWorkspace.getState().refreshEngines();
  // Project p-app has codex's "old" model of its own; the global default is Automatic.
  saveFor("p-app", { default_engine: "codex", default_model: "old", default_effort: "", default_model_options: {} });
});

it("commits one version-fenced provider/model tuple and leaves opening the control read-only", async () => {
  const user = userEvent.setup(); render(<ProjectModelDefaults query={checkout} />);
  const choose = await screen.findByRole("button", { name: "old" });
  expect(saves()).toHaveLength(0);
  const held = holds(checkout);
  await pick(user, "old", "chosen");
  await waitFor(() => expect(saves()).toHaveLength(1));
  expect(saves()[0]).toEqual({ scope: checkout, membership_version: held.version, if_match: held.revisions, edits: tuple("codex", "chosen") });
  expect(choose).toBeDefined();
});

it("retains a conflicting model draft and requires explicit reload before retry with fresh revisions", async () => {
  const user = userEvent.setup();
  render(<ProjectModelDefaults query={checkout} />);
  await screen.findByRole("button", { name: "old" });
  // Another window saves the global default after this page read the rows, so the page's revisions are out of date.
  seedPreference(mock, "default_model", "haiku");
  await pick(user, "old", "chosen");
  expect(await screen.findByRole("alert")).toHaveTextContent("Settings changed. Reload before saving.");
  expect(screen.getByRole("button", { name: "chosen" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Retry" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Retry" })).toBeEnabled());
  expect(saves()).toHaveLength(1);
  const fresh = holds(checkout);
  await user.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(saves()).toHaveLength(2));
  expect(saves()[1]?.if_match).toEqual(fresh.revisions);
  expect(saves()[1]?.edits.default_model).toEqual({ op: "set", value: "chosen" });
});

it("inherits the whole tuple together while Automatic sets every key explicitly", async () => {
  // The global default is the same model, so the choice stays after the project's own is dropped.
  seedPreference(mock, "default_engine", "codex");
  seedPreference(mock, "default_model", "old");
  const user = userEvent.setup(); render(<ProjectModelDefaults query={checkout} />);
  await user.click(await screen.findByRole("button", { name: "Inherit" }));
  await waitFor(() => expect(saves()).toHaveLength(1));
  expect(saves()[0]?.edits).toEqual(every({ op: "inherit" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Automatic" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Automatic" }));
  await waitFor(() => expect(saves()).toHaveLength(2));
  expect(saves()[1]?.edits).toEqual(tuple("", ""));
});

it("does not write on read and resets only captured tuple overrides", async () => {
  // Two projects hold their own values for different keys; the reset names exactly those two.
  saveFor("p-site", { default_model: "b-model", default_effort: "high" });
  const user = userEvent.setup(); render(<ProjectModelDefaults query={{}} />);
  await user.click(await screen.findByRole("button", { name: "Reset all" }));
  await waitFor(() => expect(saves()).toHaveLength(1));
  expect(saves()[0]?.edits).toEqual(every({ op: "reset_overrides", project_ids: ["p-app", "p-site"] }));
});

it("shows Mixed for checkouts that differ and writes nothing on read", async () => {
  mock.groupProjects(["p-app", "p-site"]);
  saveFor("p-site", { default_engine: "codex", default_model: "b-model" });
  render(<ProjectModelDefaults query={together} />);
  expect((await screen.findAllByRole("button", { name: "Mixed" }))[0]).toBeVisible();
  expect(saves()).toHaveLength(0);
});

it("never applies a late old-scope save to a different mounted selection", async () => {
  saveFor("p-site", { default_engine: "codex", default_model: "b-model" });
  const slow = holdFetch("PUT", "/api/settings/preferences");
  const user = userEvent.setup(); const { rerender } = render(<ProjectModelDefaults query={checkout} />);
  await pick(user, "old", "chosen");
  rerender(<ProjectModelDefaults query={checkoutOf("p-site")} />);
  expect(await screen.findByRole("button", { name: "b-model" })).toBeEnabled();
  await act(async () => slow.release());
  await waitFor(() => expect(mock.handle("GET", "/api/preferences?project_id=p-app").body).toContainEqual(expect.objectContaining({ key: "default_model", value: "chosen" })));
  expect(screen.getByRole("button", { name: "b-model" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "chosen" })).not.toBeInTheDocument();
});

it("shows the saved unlisted model ID instead of substituting a catalog default label", async () => {
  saveFor("p-app", { default_model: "saved-unlisted" });
  render(<ProjectModelDefaults query={checkout} />);
  expect(await screen.findByRole("button", { name: "saved-unlisted" })).toBeVisible();
  expect(saves()).toHaveLength(0);
});

it("names an unavailable saved provider instead of presenting the representative as the saved choice", async () => {
  saveFor("p-app", { default_engine: "removed-provider", default_model: "saved-model" });
  render(<ProjectModelDefaults query={checkout} />);
  expect(await screen.findByRole("button", { name: "removed-provider / saved-model" })).toBeVisible();
  expect(saves()).toHaveLength(0);
});

it("saves effort and options with the default model and clears them when the model changes", async () => {
  saveFor("p-app", { default_effort: "low", default_model_options: { service_tier: "fast" } });
  const user = userEvent.setup(); render(<ProjectModelDefaults query={checkout} />);
  const effort = await screen.findByRole("combobox", { name: "Default Effort" });
  expect(effort).toHaveValue("low");
  expect(screen.getByRole("combobox", { name: "Default Speed" })).toHaveValue("fast");
  await user.selectOptions(effort, "high");
  await waitFor(() => expect(saves()).toHaveLength(1));
  expect(saves()[0]?.edits).toEqual(tuple("codex", "old", "high", { service_tier: "fast" }));
  await waitFor(() => expect(screen.getByRole("combobox", { name: "Default Speed" })).toBeEnabled());
  await user.selectOptions(screen.getByRole("combobox", { name: "Default Speed" }), "");
  await waitFor(() => expect(saves()).toHaveLength(2));
  expect(saves()[1]?.edits).toEqual(tuple("codex", "old", "high", {}));
  await waitFor(() => expect(screen.getByRole("button", { name: "old" })).toBeEnabled());
  await pick(user, "old", "chosen");
  await waitFor(() => expect(saves()).toHaveLength(3));
  expect(saves()[2]?.edits).toEqual(tuple("codex", "chosen"));
});

it("offers no effort or options until a model is chosen, and shows Mixed effort without writing", async () => {
  saveFor("p-app", { default_model: "" });
  const { unmount } = render(<ProjectModelDefaults query={checkout} />);
  expect(await screen.findByText("Choose a model to set its effort and options.")).toBeVisible();
  expect(screen.queryByRole("combobox", { name: "Default Effort" })).not.toBeInTheDocument();
  unmount();
  mock.groupProjects(["p-app", "p-site"]);
  saveFor("p-app", { default_model: "old", default_effort: "low" });
  saveFor("p-site", { default_engine: "codex", default_model: "old", default_effort: "high" });
  render(<ProjectModelDefaults query={together} />);
  expect(await screen.findByRole("combobox", { name: "Default Effort" })).toHaveValue("__mixed__");
  expect(saves()).toHaveLength(0);
});
