import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, request } from "../../../api/client";
import type { BrowserProfile } from "../../../api/types";
import { useUi } from "../../../product/state/ui";
import { BrowserProfilesSection } from "./BrowserProfiles";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const profile = (id: string, name: string, in_use = false): BrowserProfile => ({ id, name, in_use });
let stored: BrowserProfile[] = [];
const writes: { method: string; path: string; body?: unknown }[] = [];
let refuse: ((method: string, path: string) => ApiFailure | null) | null = null;
let listFails = false;

beforeEach(() => {
  stored = [profile("p1", "Work"), profile("p2", "Client demo", true), profile("p3", "Staging")];
  writes.length = 0;
  refuse = null;
  listFails = false;
  useUi.setState({ toasts: [] });
  vi.mocked(request).mockImplementation(async (method: string, path: string, body?: unknown) => {
    if (method === "GET" && path === "/api/browser-profiles") {
      if (listFails) throw new ApiFailure("unavailable", "The service did not answer.", 503);
      return stored;
    }
    writes.push({ method, path, body });
    const refused = refuse?.(method, path);
    if (refused) throw refused;
    if (method === "POST" && path === "/api/browser-profiles") {
      const made = profile(`p${stored.length + 1}`, (body as { name: string }).name);
      stored = [...stored, made];
      return made;
    }
    return undefined;
  });
});
afterEach(() => vi.clearAllMocks());

describe("Settings › Browser", () => {
  it("lists each profile with whether a tab has it open, and offers Clear and Delete but no Rename", async () => {
    render(<BrowserProfilesSection />);
    const table = await screen.findByRole("table", { name: "Browser profiles" });
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText("Work")).toBeVisible();
    expect(within(rows[0]!).getByText("Not open")).toBeVisible();
    expect(within(rows[1]!).getByText("Open in a tab")).toBeVisible();
    expect(within(rows[1]!).getByRole("button", { name: "Clear Client demo…" })).toBeEnabled();
    expect(within(rows[2]!).getByRole("button", { name: "Delete Staging…" })).toBeEnabled();
    expect(screen.queryByText(/rename/i)).toBeNull();
    expect(screen.getByRole("button", { name: "New profile" })).toBeVisible();
  });

  it("says there are no profiles yet, with New profile, rather than an empty table", async () => {
    stored = [];
    render(<BrowserProfilesSection />);
    expect(await screen.findByText("No profiles yet")).toBeVisible();
    expect(screen.getByText(/Tabs start clean and forget everything when they close/)).toBeVisible();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.getByRole("button", { name: "New profile" })).toBeVisible();
  });

  it("says the list could not be read, never an empty list, and reads again on Try again", async () => {
    const user = userEvent.setup();
    listFails = true;
    render(<BrowserProfilesSection />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Couldn’t read the browser profiles.");
    expect(alert).toHaveTextContent("The service did not answer.");
    expect(screen.queryByText("No profiles yet")).toBeNull();
    expect(screen.queryByRole("table")).toBeNull();
    listFails = false;
    await user.click(within(alert).getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Work")).toBeVisible();
  });

  it("creates a profile with one write however fast Create is pressed, then lists it", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "New profile" }));
    await user.type(screen.getByRole("textbox", { name: "Profile name" }), "Research");
    const create = screen.getByRole("button", { name: "Create" });
    await user.dblClick(create);
    await waitFor(() => expect(screen.getByText("Research")).toBeVisible());
    expect(writes).toEqual([{ method: "POST", path: "/api/browser-profiles", body: { name: "Research" } }]);
    expect(screen.queryByRole("textbox", { name: "Profile name" })).toBeNull();
  });

  it("shows the service's refusal of a bad name under the field, adds nothing and lets Create try again", async () => {
    const user = userEvent.setup();
    refuse = (method) => method === "POST" ? new ApiFailure("invalid", "A profile name is 1 to 64 characters.", 400) : null;
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "New profile" }));
    const field = screen.getByRole("textbox", { name: "Profile name" });
    await user.type(field, "x".repeat(70));
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A profile name is 1 to 64 characters.");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(screen.getAllByRole("row")).toHaveLength(5);
    refuse = null;
    await user.clear(field);
    await user.type(field, "Short");
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("Short")).toBeVisible();
    expect(writes.map((write) => write.body)).toEqual([{ name: "x".repeat(70) }, { name: "Short" }]);
  });

  it("cancels a new profile without a write", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "New profile" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("textbox", { name: "Profile name" })).toBeNull();
    expect(writes).toEqual([]);
  });

  it("asks before clearing, with Cancel as the safe default, then clears once and says so in a toast", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Clear Work…" }));
    const dialog = await screen.findByRole("dialog", { name: "Clear the profile Work?" });
    expect(dialog).toHaveTextContent("The profile and its name stay. This can’t be undone.");
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: "Clear profile" }));
    await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Cleared the profile Work."]));
    expect(writes).toEqual([{ method: "POST", path: "/api/browser-profiles/p1/clear", body: undefined }]);
    expect(screen.getByText("Work")).toBeVisible();
  });

  it("deletes only after confirming, and the profile leaves the list", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Delete Staging…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete the profile Staging?" });
    expect(writes).toEqual([]);
    stored = stored.filter((entry) => entry.id !== "p3");
    await user.click(within(dialog).getByRole("button", { name: "Delete profile" }));
    await waitFor(() => expect(screen.queryByText("Staging")).toBeNull());
    expect(writes).toEqual([{ method: "DELETE", path: "/api/browser-profiles/p3", body: undefined }]);
  });

  it("shows the service's answer for a profile open in a tab as a banner above the list and removes nothing", async () => {
    const user = userEvent.setup();
    refuse = () => new ApiFailure("conflict", "Close the tab using this profile first.", 409);
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Delete Client demo…" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete profile" }));
    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("Close the tab using this profile first.");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("Client demo")).toBeVisible();
    await user.click(within(banner).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("draws a hostile or very long name as text, capped, and encodes its id in the address", async () => {
    const user = userEvent.setup();
    const hostile = "<img src=x onerror=alert(1)>" + "y".repeat(300);
    stored = [profile("a/b?c", hostile)];
    render(<BrowserProfilesSection />);
    expect(await screen.findByText(/^<img src=x onerror=alert\(1\)>y+…$/)).toBeVisible();
    expect(document.querySelector("img")).toBeNull();
    await user.click(screen.getByRole("button", { name: /^Clear .*…$/ }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Clear profile" }));
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]?.path).toBe("/api/browser-profiles/a%2Fb%3Fc/clear");
  });

  it("ignores list entries that are not profiles", async () => {
    stored = [profile("p1", "Work"), { id: 7, name: null } as unknown as BrowserProfile, null as unknown as BrowserProfile];
    render(<BrowserProfilesSection />);
    expect(await screen.findByText("Work")).toBeVisible();
    expect(screen.getAllByRole("row")).toHaveLength(2);
  });
});
