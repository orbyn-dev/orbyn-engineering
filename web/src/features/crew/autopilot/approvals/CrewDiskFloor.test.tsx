import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../api/types";
import { useConnection } from "../../../../product/state/connection";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { CrewSettingsSection } from "./CrewSettings";
import { CrewDiskFloor } from "./CrewDiskFloor";

let mock: MockApi;
const TITLE = "Free disk space Crew needs";
const ERROR = "Enter a number from 1 to 500.";
const writes = () => vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url).includes("/api/settings/preferences") && init?.method === "PUT");
const field = () => screen.getByRole("textbox", { name: TITLE });
const save = () => screen.getByRole("button", { name: "Save" });

beforeEach(() => {
  mock = installMockFetch();
  mock.reset("crew");
  useConnection.setState({ remote: false, crewPreview: true });
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" } as Project], error: null, loading: false } });
  window.location.hash = "";
  window.localStorage.clear();
});

describe("Settings › Crew › Free disk space Crew needs", () => {
  it("shows the stored default 5, the help line and a text field with GB and Save, with Planned gone", async () => {
    render(<CrewDiskFloor />);
    await waitFor(() => expect(field()).toHaveValue("5"));
    expect(screen.getByText("Crew refuses to start a worker when the disk that holds the lead’s folder has less than this free. It only refuses: it deletes nothing to make room. A whole number of GB, from 1 to 500. Default 5 GB.")).toBeVisible();
    expect(field()).toHaveAttribute("type", "text");
    expect(field()).toHaveAttribute("placeholder", "Enter GB");
    expect(screen.getByText("GB")).toBeVisible();
    expect(save()).toBeDisabled();
    expect(screen.queryByText("Planned")).toBeNull();
  });

  it.each([
    ["", "empty"],
    ["five", "text"],
    ["0", "zero"],
    ["-2", "negative"],
    ["600", "too large"],
    ["5.5", "fractional"],
    ["5 GB", "a unit"],
  ])("refuses %s (%s): the one sentence under the row and no write", async (typed) => {
    const user = userEvent.setup();
    render(<CrewDiskFloor />);
    await waitFor(() => expect(field()).toHaveValue("5"));
    await user.clear(field());
    if (typed) await user.type(field(), typed);
    expect(await screen.findByRole("alert")).toHaveTextContent(ERROR);
    expect(save()).toBeDisabled();
    await user.click(save());
    expect(writes()).toHaveLength(0);
  });

  it("writes a valid whole number, keeps the typed value and says Saved with Save off", async () => {
    const user = userEvent.setup();
    render(<CrewDiskFloor />);
    await waitFor(() => expect(field()).toHaveValue("5"));
    await user.clear(field());
    await user.type(field(), "8");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(save()).toBeEnabled();
    await user.click(save());
    await waitFor(() => expect(writes()).toHaveLength(1));
    const [url, init] = writes()[0]!;
    expect(String(url)).toBe("/api/settings/preferences");
    expect(JSON.parse(String(init?.body))).toMatchObject({ edits: { crew_min_free_gb: { op: "set", value: 8 } } });
    expect(new Headers(init?.headers).get("x-orbyn-person")).toBeTruthy();
    expect(await screen.findByText("Saved")).toBeVisible();
    expect(field()).toHaveValue("8");
    expect(save()).toBeDisabled();
  });

  it("keeps the typed value and Save on when the save fails, shows the service's own words, and leaves the floor unchanged", async () => {
    const user = userEvent.setup();
    const message = "Couldn’t reach the Orbyn service. The floor was not changed.";
    render(<CrewDiskFloor />);
    await waitFor(() => expect(field()).toHaveValue("5"));
    // Queue the failure after the row's own read, so the save is the request that fails.
    mock.fail("/api/settings/preferences", 1, 502, "service_unreachable", message);
    await user.clear(field());
    await user.type(field(), "8");
    await user.click(save());
    expect(await screen.findByRole("alert")).toHaveTextContent(message);
    expect(field()).toHaveValue("8");
    expect(save()).toBeEnabled();
    expect(screen.queryByText("Saved")).toBeNull();
  });

  it("is a row of Settings › Crew, before Worker provider", async () => {
    render(<CrewSettingsSection />);
    const input = await screen.findByRole("textbox", { name: TITLE });
    const provider = await screen.findByRole("region", { name: "Worker provider" });
    expect(input.compareDocumentPosition(provider) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
