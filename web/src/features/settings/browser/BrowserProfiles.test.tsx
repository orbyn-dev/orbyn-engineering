import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { BrowserProfile } from "../../../api/types";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { wireWrites } from "../../../wire.test-support";
import { BrowserProfilesSection } from "./BrowserProfiles";

const profile = (id: string, name: string, in_use = false): BrowserProfile => ({ id, name, in_use });
const internal = "Orbyn hit an internal error. Check the service log.";
let mock: MockApi;

/** The service holds these profiles: one with a tab open on it. */
const seed = (rows: BrowserProfile[] = [profile("p1", "Work"), profile("p2", "Client demo", true), profile("p3", "Staging")]) => {
  mock.reset("browser-profiles");
  mock.setBrowserProfiles(rows);
};

beforeEach(() => {
  mock = installMockFetch();
  seed();
  useUi.setState({ toasts: [] });
});

describe("Settings › Browser", () => {
  it("lists each profile with whether a tab has it open, and offers Rename, Clear and Delete", async () => {
    render(<BrowserProfilesSection />);
    const table = await screen.findByRole("table", { name: "Browser profiles" });
    const rows = within(table).getAllByRole("row").slice(1);
    expect(rows).toHaveLength(3);
    expect(within(rows[0]!).getByText("Work")).toBeVisible();
    expect(within(rows[0]!).getByText("Not open")).toBeVisible();
    expect(within(rows[1]!).getByText("Open in a tab")).toBeVisible();
    expect(within(rows[1]!).getByRole("button", { name: "Clear Client demo…" })).toBeEnabled();
    expect(within(rows[2]!).getByRole("button", { name: "Delete Staging…" })).toBeEnabled();
    expect(within(rows[0]!).getByRole("button", { name: "Rename Work…" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "New profile" })).toBeVisible();
  });

  it("says there are no profiles yet, with New profile, rather than an empty table", async () => {
    seed([]);
    render(<BrowserProfilesSection />);
    expect(await screen.findByText("No profiles yet")).toBeVisible();
    expect(screen.getByText(/Tabs start clean and forget everything when they close/)).toBeVisible();
    expect(screen.queryByRole("table")).toBeNull();
    expect(screen.getByRole("button", { name: "New profile" })).toBeVisible();
  });

  it("says the list could not be read, never an empty list, and reads again on Try again", async () => {
    const user = userEvent.setup();
    mock.fail("/api/browser-profiles", 1, 500, "internal", internal);
    render(<BrowserProfilesSection />);
    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("Couldn’t read the browser profiles.");
    expect(alert).toHaveTextContent(internal);
    expect(screen.queryByText("No profiles yet")).toBeNull();
    expect(screen.queryByRole("table")).toBeNull();
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
    expect(wireWrites()).toEqual([{ method: "POST", path: "/api/browser-profiles", body: { name: "Research" } }]);
    expect(screen.queryByRole("textbox", { name: "Profile name" })).toBeNull();
  });

  it("shows the service's refusal of a bad name under the field, adds nothing and lets Create try again", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "New profile" }));
    mock.fail("/api/browser-profiles", 1, 400, "invalid_input", "A profile name is 1 to 64 characters.");
    const field = screen.getByRole("textbox", { name: "Profile name" });
    await user.type(field, "x".repeat(70));
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("A profile name is 1 to 64 characters.");
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(screen.getAllByRole("row")).toHaveLength(5);
    await user.clear(field);
    await user.type(field, "Short");
    await user.click(screen.getByRole("button", { name: "Create" }));
    expect(await screen.findByText("Short")).toBeVisible();
    expect(wireWrites().map((write) => write.body)).toEqual([{ name: "x".repeat(70) }, { name: "Short" }]);
  });

  it("renames a profile with one write on Save, then says so in a toast and the list", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Rename Work…" }));
    const dialog = await screen.findByRole("dialog", { name: "Rename the profile Work?" });
    const field = within(dialog).getByRole("textbox", { name: "Profile name" });
    expect(field).toHaveValue("Work");
    await user.clear(field);
    await user.type(field, " Home ");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByText("Home")).toBeVisible());
    expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Renamed the profile Work to Home."]);
    expect(wireWrites()).toEqual([{ method: "POST", path: "/api/browser-profiles/p1/rename", body: { name: "Home" } }]);
    expect(screen.queryByText("Work")).toBeNull();
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("closes an unchanged name without a write or a toast, and treats extra spaces as unchanged", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Rename Work…" }));
    await user.click(within(await screen.findByRole("dialog", { name: "Rename the profile Work?" })).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(wireWrites()).toEqual([]);
    expect(useUi.getState().toasts).toEqual([]);

    await user.click(screen.getByRole("button", { name: "Rename Work…" }));
    const dialog = await screen.findByRole("dialog", { name: "Rename the profile Work?" });
    const field = within(dialog).getByRole("textbox", { name: "Profile name" });
    await user.clear(field);
    await user.type(field, "Work  ");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(wireWrites()).toEqual([]);
    expect(useUi.getState().toasts).toEqual([]);
    expect(screen.getByText("Work")).toBeVisible();
  });

  it("the dev mock validates a rename like the service: 1 to 64 characters and a known id", () => {
    expect(mock.handle("POST", "/api/browser-profiles/p1/rename", { name: "x".repeat(70) })).toEqual({ status: 400, body: { code: "invalid_input", message: "A profile name is 1 to 64 characters." } });
    expect(mock.handle("POST", "/api/browser-profiles/p1/rename", { name: "  " })).toEqual({ status: 400, body: { code: "invalid_input", message: "A profile name is 1 to 64 characters." } });
    expect(mock.handle("POST", "/api/browser-profiles/missing/rename", { name: "Home" })).toEqual({ status: 404, body: { code: "not_found", message: "browser profile not found" } });
    expect(mock.handle("POST", "/api/browser-profiles/p1/rename", { name: " Home " })).toEqual({ status: 200, body: { id: "p1", name: "Home", in_use: false } });
  });

  it("the dev mock counts Unicode characters after trimming, so 64 emoji are a valid name", () => {
    // 64 emoji are 128 UTF-16 units: the service trims, then counts characters (profiles.rs:20 chars().count()), and accepts them.
    const boundary = "🦀".repeat(64);
    expect(mock.handle("POST", "/api/browser-profiles/p1/rename", { name: `\u2003${boundary}\u3000` })).toEqual({ status: 200, body: { id: "p1", name: boundary, in_use: false } });
    expect(mock.handle("POST", "/api/browser-profiles/p1/rename", { name: `${boundary}🦀` })).toEqual({ status: 400, body: { code: "invalid_input", message: "A profile name is 1 to 64 characters." } });
  });

  it("shows the service’s refusal of a bad name in the dialog, keeps the old name and lets Save try again", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Rename Work…" }));
    const dialog = await screen.findByRole("dialog", { name: "Rename the profile Work?" });
    mock.fail("/api/browser-profiles/p1/rename", 1, 400, "invalid_input", "A profile name is 1 to 64 characters.");
    const field = within(dialog).getByRole("textbox", { name: "Profile name" });
    await user.clear(field);
    await user.type(field, "x".repeat(70));
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("A profile name is 1 to 64 characters.");
    expect(within(dialog).getByRole("textbox", { name: "Profile name" })).toHaveValue("x".repeat(70));
    expect(screen.getByText("Work")).toBeVisible();
    await user.clear(field);
    await user.type(field, "Home");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    await waitFor(() => expect(screen.getByText("Home")).toBeVisible());
    expect(wireWrites().map((write) => write.body)).toEqual([{ name: "x".repeat(70) }, { name: "Home" }]);
  });

  it("cancels a new profile without a write", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "New profile" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("textbox", { name: "Profile name" })).toBeNull();
    expect(wireWrites()).toEqual([]);
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
    expect(wireWrites()).toEqual([{ method: "POST", path: "/api/browser-profiles/p1/clear" }]);
    expect(screen.getByText("Work")).toBeVisible();
  });

  it("deletes only after confirming, and the profile leaves the list", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Delete Staging…" }));
    const dialog = await screen.findByRole("dialog", { name: "Delete the profile Staging?" });
    expect(wireWrites()).toEqual([]);
    await user.click(within(dialog).getByRole("button", { name: "Delete profile" }));
    await waitFor(() => expect(screen.queryByText("Staging")).toBeNull());
    expect(wireWrites()).toEqual([{ method: "DELETE", path: "/api/browser-profiles/p3" }]);
  });

  it("shows the service's answer for a profile open in a tab as a banner above the list and removes nothing", async () => {
    const user = userEvent.setup();
    render(<BrowserProfilesSection />);
    await user.click(await screen.findByRole("button", { name: "Delete Client demo…" }));
    mock.fail("/api/browser-profiles/p2", 1, 400, "invalid_input", "That profile is open in another tab.");
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Delete profile" }));
    const banner = await screen.findByRole("alert");
    expect(banner).toHaveTextContent("That profile is open in another tab.");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.getByText("Client demo")).toBeVisible();
    await user.click(within(banner).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("draws a hostile or very long name as text, capped, and encodes its id in the address", async () => {
    const user = userEvent.setup();
    const hostile = "<img src=x onerror=alert(1)>" + "y".repeat(300);
    seed([profile("a/b?c", hostile)]);
    render(<BrowserProfilesSection />);
    expect(await screen.findByText(/^<img src=x onerror=alert\(1\)>y+…$/)).toBeVisible();
    expect(document.querySelector("img")).toBeNull();
    await user.click(screen.getByRole("button", { name: /^Clear .*…$/ }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Clear profile" }));
    await waitFor(() => expect(wireWrites()).toHaveLength(1));
    expect(wireWrites()[0]?.path).toBe("/api/browser-profiles/a%2Fb%3Fc/clear");
  });

  it("ignores list entries that are not profiles", async () => {
    seed([profile("p1", "Work"), { id: 7, name: null } as unknown as BrowserProfile, null as unknown as BrowserProfile]);
    render(<BrowserProfilesSection />);
    expect(await screen.findByText("Work")).toBeVisible();
    expect(screen.getAllByRole("row")).toHaveLength(2);
  });
});
