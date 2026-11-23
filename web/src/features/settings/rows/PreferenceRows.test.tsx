import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { PreferenceView } from "../../../api/types";
import { installMockFetch } from "../../../test-support";
import { PreferenceGroups } from "./PreferenceRows";
import { preferenceWrites } from "./preferenceWrites.test-support";

const groups = [{ title: "Source control", rows: [{ key: "auto_pull", title: "Auto pull", inheritance: true }] }];
/** The two checkouts the mock service lists, standing in for the two projects the old fixture invented. */
const A = "p-app";
const B = "p-site";
const UNAVAILABLE = ["unavailable", "The service is unavailable."] as const;
let mock: MockApi;

/** What the service stores for Auto pull at the device (no project) or for one project. */
const stored = (project?: string): PreferenceView => {
  const views = mock.handle("GET", `/api/preferences${project ? `?project_id=${project}` : ""}`).body as PreferenceView[];
  return views.find((view) => view.key === "auto_pull")!;
};
const overrideSet = (project: string) => stored(project).source === "project";

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  mock.handle("PUT", "/api/preferences", { values: { auto_pull: true } });
  mock.handle("PUT", `/api/projects/${A}/preferences`, { values: { auto_pull: false } });
  mock.handle("PUT", `/api/projects/${B}/preferences`, { values: { auto_pull: true } });
});

describe("project preference inheritance", () => {
  it("removes only the selected override and follows later global changes", async () => {
    const user = userEvent.setup();
    const first = render(<PreferenceGroups groups={groups} projectId={A} />);
    expect(await screen.findByRole("switch", { name: "Auto pull" })).not.toBeChecked();
    await user.click(screen.getByRole("button", { name: "Reset Auto pull to default" }));
    await waitFor(() => expect(screen.getByRole("switch")).toBeChecked());
    expect(preferenceWrites().map((write) => [write.scope.selection, write.edits])).toEqual([
      [expect.objectContaining({ kind: "checkout", project_id: A }), { auto_pull: { op: "inherit" } }],
    ]);
    expect(overrideSet(A)).toBe(false);
    expect(overrideSet(B)).toBe(true);
    expect(stored().value).toBe(true);
    first.unmount();
    mock.handle("PUT", "/api/preferences", { values: { auto_pull: false } });
    render(<PreferenceGroups groups={groups} projectId={A} />);
    // The kept answer draws first; the fresh read then follows the global change.
    await waitFor(() => expect(screen.getByRole("switch")).not.toBeChecked());
    expect(screen.queryByRole("button", { name: "Reset Auto pull to default" })).toBeNull();
  });

  it("keeps an equal-valued explicit override resettable and explains all three layers", async () => {
    const user = userEvent.setup();
    render(<PreferenceGroups groups={groups} projectId={B} />);
    await screen.findByRole("switch");
    expect(screen.getByRole("button", { name: "Reset Auto pull to default" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Reset Auto pull to default" })).toHaveAttribute("title", "Reset to inherited value");
    const source = screen.getByRole("button", { name: "Set for this project" });
    source.focus(); await user.keyboard("{Enter}");
    const popup = await screen.findByRole("dialog", { name: "Auto pull" });
    expect(within(popup).getByText("Project").closest("li")).toHaveTextContent("On");
    expect(within(popup).getByText("Environment").closest("li")).toHaveTextContent("On");
    expect(within(popup).getByText("Default").closest("li")).toHaveTextContent("Off");
    expect(within(popup).queryByText("t3.json")).toBeNull();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Auto pull" })).toBeNull(); expect(source).toHaveFocus();
    expect(preferenceWrites()).toHaveLength(0);
  });

  it("retains an override after a failed reset", async () => {
    const user = userEvent.setup();
    render(<PreferenceGroups groups={groups} projectId={A} />);
    await screen.findByRole("switch");
    mock.fail("/api/settings/preferences", 1, 503, ...UNAVAILABLE, "PUT");
    await user.click(screen.getByRole("button", { name: "Reset Auto pull to default" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The service is unavailable.");
    expect(screen.getByRole("switch")).not.toBeChecked(); expect(stored(A)).toMatchObject({ value: false, source: "project" });
    expect(screen.getByRole("button", { name: "Reset Auto pull to default" })).toBeEnabled();
  });

  it("does not fabricate inherited values when the global layer cannot be read", async () => {
    mock.fail("/api/settings/preferences/query", 1, 503, ...UNAVAILABLE);
    render(<PreferenceGroups groups={groups} projectId={A} />);
    expect(await screen.findByText("Couldn't load settings")).toBeVisible();
    expect(screen.queryByRole("switch")).toBeNull();
    expect(screen.queryByRole("button", { name: /Set for this project|From this device|Default/ })).toBeNull();
    await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("switch")).not.toBeChecked();
    expect(screen.getByRole("button", { name: "Reset Auto pull to default" })).toBeEnabled();
  });

  it("does not show an earlier project's pending save under the replacement project", async () => {
    const user = userEvent.setup();
    mock.handle("PUT", `/api/projects/${B}/preferences`, { values: { auto_pull: false } });
    // The network holds the save back until the test lets it through.
    let release = () => {};
    const held = new Promise<void>((resolve) => { release = resolve; });
    const through = vi.mocked(fetch).getMockImplementation()!;
    vi.mocked(fetch).mockImplementation(async (input, init) => {
      if (init?.method === "PUT" && String(input) === "/api/settings/preferences") await held;
      return through(input, init);
    });
    const view = render(<PreferenceGroups groups={groups} projectId={A} />);
    await user.click(await screen.findByRole("switch"));
    await waitFor(() => expect(preferenceWrites()).toHaveLength(1));
    view.rerender(<PreferenceGroups groups={groups} projectId={B} />);
    expect(await screen.findByRole("switch")).not.toBeChecked();
    await act(async () => { release(); await held; });
    await waitFor(() => expect(stored(A).value).toBe(true));
    expect(screen.getByRole("switch")).not.toBeChecked();
    expect(preferenceWrites()[0]?.scope.selection).toMatchObject({ kind: "checkout", project_id: A });
    expect(stored(B).value).toBe(false);
  });
});
