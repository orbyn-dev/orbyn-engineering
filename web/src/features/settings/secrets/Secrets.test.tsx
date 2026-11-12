import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { RedactionSettings } from "../../../generated/RedactionSettings";
import { installMockFetch } from "../../../test-support";
import { SettingsPage } from "../SettingsPage";
import { SecretsSection } from "./SecretsSection";

// Values that look like secrets are built here from parts and never written as one literal.
const part = (...pieces: string[]) => pieces.join("");

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

const view = () => mock.handle("GET", "/api/settings/secrets").body as RedactionSettings;
const open = async () => {
  render(<SecretsSection />);
  await screen.findByText("Where values are hidden");
};
const row = (title: string) => screen.getByRole("switch", { name: title });

describe("Settings › Secrets", () => {
  it("lists where values are hidden, the formats and the patterns, with the ledger locked on", async () => {
    await open();
    expect(row("Terminal blocks")).toBeChecked();
    expect(row("What agents read")).toBeChecked();
    expect(screen.getByText("Crew ledger and Crew objects")).toBeVisible();
    expect(screen.getByText("Always on")).toBeVisible();
    expect(screen.queryByRole("switch", { name: /Crew ledger/ })).toBeNull();
    expect(screen.getByText("None. Redaction is on in every project.")).toBeVisible();
    for (const format of view().formats) expect(row(format.label)).toBeChecked();
    expect(screen.getByText("Environment values named like a secret", { selector: ".setting-line-title" })).toBeVisible();
    expect(screen.getByRole("button", { name: "Add pattern" })).toBeVisible();
    expect(screen.queryByRole("table", { name: "Your patterns" })).toBeNull();
  });

  it("is a Settings category beside Crew and finds its rows in search", async () => {
    const user = userEvent.setup();
    render(<SettingsPage section="secrets" onClose={() => undefined} />);
    expect(await screen.findByRole("link", { name: "Secrets" })).toHaveAttribute("aria-current", "page");
    await user.type(screen.getByRole("searchbox", { name: "Search settings" }), "agents read");
    expect(await screen.findByRole("link", { name: "What agents read · Secrets" })).toBeVisible();
  });

  it("turns the terminal mask off at once and back on", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(row("Terminal blocks"));
    await waitFor(() => expect(row("Terminal blocks")).not.toBeChecked());
    expect(view().terminal).toBe(false);
    expect(view().agents).toBe(true);
    await user.click(row("Terminal blocks"));
    await waitFor(() => expect(view().terminal).toBe(true));
  });

  it("asks once before turning a format off, says what changes, and Cancel changes nothing", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(row("Slack token"));
    const dialog = await screen.findByRole("dialog", { name: "Turn off Slack tokens?" });
    expect(within(dialog).getByText(/Values that look like Slack tokens will be shown in terminal blocks and may be included in what agents read/)).toBeVisible();
    expect(within(dialog).getByText(/Values already hidden stay hidden/)).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(row("Slack token")).toBeChecked();
    expect(view().formats.find((format) => format.id === "slack_token")?.enabled).toBe(true);

    await user.click(row("Slack token"));
    await user.click(within(await screen.findByRole("dialog", { name: "Turn off Slack tokens?" })).getByRole("button", { name: "Turn off" }));
    await waitFor(() => expect(row("Slack token")).not.toBeChecked());
    expect(view().formats.find((format) => format.id === "slack_token")?.enabled).toBe(false);
    // Turning it back on is not a widening, so it asks nothing.
    await user.click(row("Slack token"));
    await waitFor(() => expect(row("Slack token")).toBeChecked());
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("words the dialog for the named-value format without a plural", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(row("Environment values named like a secret"));
    const dialog = await screen.findByRole("dialog", { name: "Turn off Environment values named like a secret?" });
    expect(within(dialog).getByText(/Environment values named like a secret will be shown/)).toBeVisible();
  });

  it("warns before agents read real values everywhere, with Cancel as the default", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(row("What agents read"));
    const dialog = await screen.findByRole("dialog", { name: "Turn off redaction for what agents read?" });
    expect(within(dialog).getByText("Agents will see real secrets in every project.")).toBeVisible();
    expect(within(dialog).getByText(/Crew objects are not affected/)).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(dialog).getByRole("button", { name: "Turn off redaction" }));
    await waitFor(() => expect(row("What agents read")).not.toBeChecked());
    expect(view().agents).toBe(false);
    expect(view().crew_locked).toBe(true);
  });

  it("turns redaction off for one project with a warning, lists it and turns it back on", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(screen.getByRole("button", { name: "Manage" }));
    const dialog = await screen.findByRole("dialog", { name: /^Turn off redaction for / });
    expect(within(dialog).getByText("Agents will see real secrets in this project.")).toBeVisible();
    expect(within(dialog).getByText("Only this project is affected. You can turn it back on at any time.")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    const project = await within(dialog).findByRole("combobox", { name: "Project" });
    await waitFor(() => expect(within(project).getAllByRole("option").length).toBeGreaterThan(0));
    const name = (project as HTMLSelectElement).selectedOptions[0]!.textContent!;
    await user.click(within(dialog).getByRole("button", { name: "Turn off redaction" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(screen.getByText(new RegExp(`${name}\\. Agents read their real values`))).toBeVisible();
    expect(view().projects_off.map((entry) => entry.name)).toEqual([name]);

    await user.click(screen.getByRole("button", { name: "Manage" }));
    await user.click(await screen.findByRole("button", { name: `Turn redaction back on for ${name}` }));
    await waitFor(() => expect(view().projects_off).toEqual([]));
  });

  it("changes nothing for a project when the dialog is cancelled", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(screen.getByRole("button", { name: "Manage" }));
    await user.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(view().projects_off).toEqual([]);
  });

  it("says when the settings cannot be read and reads again", async () => {
    const user = userEvent.setup();
    mock.fail("/api/settings/secrets", 1);
    render(<SecretsSection />);
    expect(await screen.findByRole("alert")).toHaveTextContent("Secrets could not be read");
    await user.click(screen.getByRole("button", { name: "Try again" }));
    expect(await screen.findByText("Where values are hidden")).toBeVisible();
  });

  it("shows a refused change in words and keeps the switch where it was", async () => {
    const user = userEvent.setup();
    await open();
    mock.fail("/api/settings/secrets", 1, 500, "internal", "The change could not be saved.");
    await user.click(row("Terminal blocks"));
    expect(await screen.findByRole("alert")).toHaveTextContent("The change could not be saved.");
    expect(row("Terminal blocks")).toBeChecked();
  });
});

describe("Settings › Secrets › patterns", () => {
  const openDialog = async (user: ReturnType<typeof userEvent.setup>) => {
    await open();
    await user.click(screen.getByRole("button", { name: "Add pattern" }));
    return screen.findByRole("dialog", { name: "Add a pattern" });
  };
  const fill = async (user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement, name: string, pattern: string, sample = "") => {
    await user.type(within(dialog).getByRole("textbox", { name: "Name" }), name);
    await user.type(within(dialog).getByRole("textbox", { name: "Pattern" }), pattern.replaceAll("{", "{{").replaceAll("[", "[["));
    if (sample) await user.type(within(dialog).getByRole("textbox", { name: "Try it on some text" }), sample);
  };

  it("starts empty with Add pattern disabled until the pattern matches the sample", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog(user);
    expect(within(dialog).getByText("Nothing to test yet")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Add pattern" })).toBeDisabled();
    expect(within(dialog).getByRole("radio", { name: "Regular expression" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "Anywhere" })).toBeChecked();
    await fill(user, dialog, "Acme key", part("acme", "_[a-z]{8}"));
    expect(await within(dialog).findByText("No match")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Add pattern" })).toBeDisabled();
  });

  it("shows what would be hidden and adds the pattern to the list", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog(user);
    await fill(user, dialog, "Acme key", part("acme", "_[a-z]{8}"), part("KEY=", "acme", "_", "abcdefgh"));
    expect(await within(dialog).findByText("Matched 1 value")).toBeVisible();
    expect(within(dialog).getByLabelText("hidden value")).toHaveTextContent("•".repeat(13));
    await user.click(within(dialog).getByRole("button", { name: "Add pattern" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const table = screen.getByRole("table", { name: "Your patterns" });
    expect(within(table).getByText("Acme key")).toBeVisible();
    expect(within(table).getByText("Anywhere")).toBeVisible();
    expect(view().patterns).toHaveLength(1);
  });

  it("refuses an invalid pattern in words, keeps the text and leaves Add disabled", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog(user);
    await fill(user, dialog, "Broken", part("ab", "_([a-z]{4}"), "ab_wxyz");
    expect(await within(dialog).findByText("This isn’t a valid pattern: the group opened at position 3 is not closed.")).toBeVisible();
    const field = within(dialog).getByRole("textbox", { name: "Pattern" });
    expect(field).toHaveValue(part("ab", "_([a-z]{4}"));
    expect(field).toHaveAttribute("aria-invalid", "true");
    expect(within(dialog).getByText("Fix the pattern to see what it would hide.")).toBeVisible();
    expect(within(dialog).getByRole("button", { name: "Add pattern" })).toBeDisabled();
    expect(view().patterns).toEqual([]);
  });

  it("says a pattern is very broad with the numbers and adds it only as Add anyway", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog(user);
    await fill(user, dialog, "Everything", ".*", "npm run build");
    expect(await within(dialog).findByText(/This pattern is very broad\./)).toBeVisible();
    expect(within(dialog).getByText(/It would hide 12 of 12 lines of ordinary text/)).toBeVisible();
    expect(within(dialog).getByText("Matched 12 of 12 lines")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Add anyway" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(view().patterns.map((entry) => entry.name)).toEqual(["Everything"]);
  });

  it("reads a text pattern with * and an environment-only scope", async () => {
    const user = userEvent.setup();
    const dialog = await openDialog(user);
    await user.click(within(dialog).getByRole("radio", { name: "Text" }));
    await user.click(within(dialog).getByRole("radio", { name: "Environment values" }));
    expect(within(dialog).getByText(/\* stands for any run of characters that are not spaces/)).toBeVisible();
    await fill(user, dialog, "Internal host", "*.corp.example.com", "db1.corp.example.com");
    expect(await within(dialog).findByText("Matched 1 value")).toBeVisible();
    await user.click(within(dialog).getByRole("button", { name: "Add pattern" }));
    const table = await screen.findByRole("table", { name: "Your patterns" });
    expect(within(table).getByText("Environment values")).toBeVisible();
  });

  it("shows the service's refusal when it saves, such as a name already in the list", async () => {
    const user = userEvent.setup();
    mock.handle("POST", "/api/settings/secrets/patterns", { name: "Acme key", pattern: "acme-token", kind: "text", applies_to: "anywhere" });
    const dialog = await openDialog(user);
    await fill(user, dialog, "acme key", "other-token", "other-token");
    await within(dialog).findByText("Matched 1 value");
    await user.click(within(dialog).getByRole("button", { name: "Add pattern" }));
    expect(await within(dialog).findByText("A pattern with this name is already in the list.")).toBeVisible();
    expect(screen.getByRole("dialog", { name: "Add a pattern" })).toBeVisible();
  });

  it("edits a pattern from its menu and removes one after asking", async () => {
    const user = userEvent.setup();
    mock.handle("POST", "/api/settings/secrets/patterns", { name: "Acme key", pattern: "acme-token", kind: "text", applies_to: "anywhere" });
    await open();
    await user.click(screen.getByRole("button", { name: "Menu for Acme key" }));
    await user.click(await screen.findByRole("menuitem", { name: "Edit" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit pattern" });
    const name = within(dialog).getByRole("textbox", { name: "Name" });
    await user.clear(name);
    await user.type(name, "Acme token");
    await user.type(within(dialog).getByRole("textbox", { name: "Try it on some text" }), "my acme-token");
    await within(dialog).findByText("Matched 1 value");
    await user.click(within(dialog).getByRole("button", { name: "Save" }));
    expect(await screen.findByText("Acme token")).toBeVisible();

    await user.click(screen.getByRole("button", { name: "Menu for Acme token" }));
    await user.click(await screen.findByRole("menuitem", { name: "Remove" }));
    const confirm = await screen.findByRole("dialog", { name: "Remove Acme token?" });
    expect(within(confirm).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(confirm).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("table", { name: "Your patterns" })).toBeNull());
    expect(view().patterns).toEqual([]);
  });
});
