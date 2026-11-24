import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { navigate, useRoute } from "../../../product/state/route";
import { installMockFetch } from "../../../test-support";
import { SettingsScopeProvider, useSettingsScope } from "./SettingsScopeContext";
import { ScopeSentence } from "./ScopeSentence";

/** The service's three projects, and the worktree of the app that "settings-checkouts" adds to the first of them. */
const APP_GROUP = "repo:orbyn-engineering-app";
let mock: MockApi;

beforeEach(() => {
  mock = installMockFetch();
});

/** Adds projects until the service lists this many, as a person adds folders. */
function addProjects(total: number) {
  const have = (mock.handle("POST", "/api/settings/project-scopes", {}).body as { groups: unknown[] }).groups.length;
  for (let index = have; index < total; index += 1) mock.handle("POST", "/api/projects/add", { path: `/Users/dev/code/extra-${index}` });
}

function Probe() {
  const context = useSettingsScope();
  return <><ScopeSentence /><output data-testid="query">{JSON.stringify(context?.query)}</output>
    <button onClick={() => navigate({ name: "settings", section: "appearance" })}>Appearance</button>
    <button onClick={() => navigate({ name: "settings", section: "worktrees" })}>Worktrees</button>
    <button onClick={() => context?.select({ kind: "checkout", group_key: APP_GROUP, project_id: "p-app-fix" })}>Open physical checkout</button>
    <button>After scope</button></>;
}
function Harness() {
  const route = useRoute();
  return <SettingsScopeProvider section={route.name === "settings" ? route.section ?? "general" : "general"}><Probe /></SettingsScopeProvider>;
}

it("selects a logical project and physical checkout, retaining IDs across device pages", async () => {
  mock.reset("settings-checkouts"); window.location.hash = "#/settings/worktrees";
  const user = userEvent.setup(); render(<Harness />);
  expect(await screen.findByText("Test computer")).toBeVisible();
  await user.click(await screen.findByRole("button", { name: "Project scope: All projects" }));
  await user.click(screen.getByRole("menuitemradio", { name: "orbyn-engineering-app" }));
  await user.click(screen.getByRole("button", { name: "Open physical checkout" }));
  await waitFor(() => expect(screen.getByTestId("query")).toHaveTextContent('"project_id":"p-app-fix"'));
  expect(window.location.hash).toContain("checkout=p-app-fix");
  await user.click(screen.getByRole("button", { name: "Appearance" }));
  await waitFor(() => expect(screen.getByTestId("query")).toHaveTextContent("{}"));
  expect(screen.queryByText("Applying settings for")).not.toBeInTheDocument();
  expect(window.location.hash).toContain("checkout=p-app-fix");
  await user.click(screen.getByRole("button", { name: "Worktrees" }));
  await waitFor(() => expect(screen.getByTestId("query")).toHaveTextContent('"project_id":"p-app-fix"'));
});

it("keeps stale scope unavailable and lets the user explicitly recover", async () => {
  window.location.hash = "#/settings/worktrees?project=removed";
  const user = userEvent.setup(); render(<Harness />);
  await user.click(await screen.findByRole("button", { name: "Project scope: Unavailable project" }));
  expect(screen.getByText(/This selection is no longer available/)).toBeVisible();
  expect(screen.getByTestId("query")).toHaveTextContent('"group_key":"removed"');
  await user.click(screen.getByRole("menuitemradio", { name: "All projects" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Project scope: All projects" })).toBeEnabled());
  expect(window.location.hash).not.toContain("project=");
});

it("uses the ordinary menu for exactly six projects", async () => {
  addProjects(6);
  window.location.hash = "#/settings/worktrees";
  const user = userEvent.setup(); render(<Harness />);
  await user.click(await screen.findByRole("button", { name: "Project scope: All projects" }));
  expect(screen.getAllByRole("menuitemradio")).toHaveLength(7);
  expect(screen.queryByRole("combobox", { name: "Search projects" })).not.toBeInTheDocument();
});

it("uses a searchable picker only above six projects and Tab advances without a focus reset", async () => {
  addProjects(7);
  window.location.hash = "#/settings/worktrees";
  const user = userEvent.setup(); render(<Harness />);
  await user.click(await screen.findByRole("button", { name: "Project scope: All projects" }));
  const search = screen.getByRole("combobox", { name: "Search projects" });
  await user.type(search, "extra-6");
  expect(within(screen.getByRole("listbox", { name: "Projects" })).getAllByRole("option")).toHaveLength(1);
  await user.keyboard("{Enter}");
  await waitFor(() => expect(screen.getByRole("button", { name: "Project scope: extra-6" })).toBeEnabled());
  await user.click(screen.getByRole("button", { name: "Project scope: extra-6" }));
  await user.tab();
  await waitFor(() => expect(screen.queryByRole("combobox", { name: "Search projects" })).not.toBeInTheDocument());
  expect(screen.getByRole("button", { name: "Project scope: extra-6" })).not.toHaveFocus();
});

it("keeps a failed catalog blocked and exposes retry", async () => {
  mock.fail("/api/settings/project-scopes", 1, 503, "unavailable", "The service is unavailable.");
  window.location.hash = "#/settings/worktrees?project=mock:p-app";
  const user = userEvent.setup(); render(<Harness />);
  expect(await screen.findByRole("alert")).toHaveTextContent("The service is unavailable.");
  expect(screen.getByRole("button", { name: "Project scope: Unavailable project" })).toBeDisabled();
  await user.click(screen.getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Project scope: orbyn-engineering-app" })).toBeEnabled());
});

it("does not expose a former catalog while a new selection is loading", async () => {
  window.location.hash = "#/settings/worktrees?project=mock:p-app";
  const user = userEvent.setup(); render(<Harness />);
  await screen.findByRole("button", { name: "Project scope: orbyn-engineering-app" });
  // The network holds the next catalog back until the test lets it through.
  let release = () => {};
  const held = new Promise<void>((resolve) => { release = resolve; });
  const through = vi.mocked(fetch).getMockImplementation()!;
  vi.mocked(fetch).mockImplementation(async (input, init) => {
    if (String(input) === "/api/settings/project-scopes") await held;
    return through(input, init);
  });
  await user.click(screen.getByRole("button", { name: "Project scope: orbyn-engineering-app" }));
  await user.click(screen.getByRole("menuitemradio", { name: "All projects" }));
  expect(await screen.findByRole("button", { name: "Project scope: Loading projects…" })).toBeDisabled();
  release();
  await waitFor(() => expect(screen.getByRole("button", { name: "Project scope: All projects" })).toBeEnabled());
});

it("keeps an unsupported machine unavailable until an explicit local selection", async () => {
  window.location.hash = "#/settings/worktrees?machine=missing&project=mock:p-app";
  const user = userEvent.setup(); render(<Harness />);
  await user.click(await screen.findByRole("button", { name: "Project scope: Unavailable project" }));
  expect(screen.getByTestId("query")).toHaveTextContent("unavailable:machine:missing");
  expect(window.location.hash).toContain("machine=missing");
  await user.click(screen.getByRole("menuitemradio", { name: "orbyn-engineering-app" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Project scope: orbyn-engineering-app" })).toBeEnabled());
  expect(window.location.hash).not.toContain("machine=");
});

it("names a physical checkout without marking its whole logical group selected", async () => {
  mock.reset("settings-checkouts"); window.location.hash = `#/settings/worktrees?project=${APP_GROUP}&checkout=p-app-fix`;
  const user = userEvent.setup(); render(<Harness />);
  const picker = await screen.findByRole("button", { name: "Project scope: orbyn-engineering-app (fix-login)" });
  await user.click(picker);
  expect(screen.getByRole("menuitemradio", { name: "orbyn-engineering-app" })).toHaveAttribute("aria-checked", "false");
  expect(screen.getByRole("menuitemradio", { name: "All projects" })).toHaveAttribute("aria-checked", "false");
  expect(screen.getByTestId("query")).toHaveTextContent('"project_id":"p-app-fix"');
  await user.click(screen.getByRole("menuitemradio", { name: "orbyn-engineering-app" }));
  await waitFor(() => expect(screen.getByRole("button", { name: "Project scope: orbyn-engineering-app" })).toBeEnabled());
  expect(screen.getByTestId("query")).toHaveTextContent('"kind":"project_group"');
  expect(window.location.hash).not.toContain("checkout=");
});

it("does not label a missing physical checkout as its logical group", async () => {
  mock.reset("settings-checkouts"); window.location.hash = `#/settings/worktrees?project=${APP_GROUP}&checkout=removed`;
  render(<Harness />);
  expect(await screen.findByRole("button", { name: "Project scope: Unavailable project" })).toBeEnabled();
  expect(screen.getByTestId("query")).toHaveTextContent('"project_id":"removed"');
  expect(window.location.hash).toContain("checkout=removed");
});
