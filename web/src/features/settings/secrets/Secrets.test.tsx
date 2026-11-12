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
    // No switch for what agents read: it is per project, through Manage.
    expect(screen.queryByRole("switch", { name: "What agents read" })).toBeNull();
    expect(screen.getByText("What agents read")).toBeVisible();
    expect(screen.getAllByRole("button", { name: "Manage" })).toHaveLength(1);
    expect(screen.getByText("On by default. Switched off per project.")).toBeVisible();
    expect(screen.getByText("Crew ledger and Crew objects")).toBeVisible();
    expect(screen.getByText(/Crew blocks every recognised format/)).toBeVisible();
    expect(screen.queryByText(/Reveal/)).toBeNull();
    expect(screen.queryByText(/apply to new output/)).toBeNull();
    expect(screen.getByText(/Today they apply to the Crew ledger when you read it, old rows included/)).toBeVisible();
    expect(screen.getByText("Always on")).toBeVisible();
    expect(screen.queryByRole("switch", { name: /Crew ledger/ })).toBeNull();
    expect(screen.getByText("None. Redaction is on in every project.")).toBeVisible();
    // What is not built is said so, not promised.
    expect(screen.getByRole("status")).toHaveTextContent("not applied yet");
    expect(screen.getAllByText("Not applied yet")).toHaveLength(2);
    expect(screen.getAllByText(/Not applied yet to terminal blocks/)).toHaveLength(1);
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

  it("lists projects with their state, warns before turning one off and turns it back on plainly", async () => {
    const user = userEvent.setup();
    await open();
    await user.click(screen.getByRole("button", { name: "Manage" }));
    const list = await screen.findByRole("dialog", { name: "Projects with redaction off" });
    const buttons = await within(list).findAllByRole("button", { name: /^Turn off redaction for / });
    expect(within(list).getAllByText(/Redaction on/).length).toBe(buttons.length);
    const name = buttons[0]!.getAttribute("aria-label")!.replace("Turn off redaction for ", "");
    await user.click(buttons[0]!);
    const warning = await screen.findByRole("dialog", { name: `Turn off redaction for ${name}?` });
    expect(within(warning).getByText("Agents will see real secrets in this project.")).toBeVisible();
    expect(within(warning).getByText("Only this project is affected. You can turn it back on at any time.")).toBeVisible();
    expect(within(warning).getByText(/Not applied yet: agents are not protected today/)).toBeVisible();
    expect(within(warning).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(warning).getByRole("button", { name: "Cancel" }));
    expect(view().projects_off).toEqual([]);
    await user.click(within(await screen.findByRole("dialog", { name: "Projects with redaction off" })).getAllByRole("button", { name: /^Turn off redaction for / })[0]!);
    await user.click(within(await screen.findByRole("dialog", { name: `Turn off redaction for ${name}?` })).getByRole("button", { name: "Turn off redaction" }));
    await waitFor(() => expect(view().projects_off.map((entry) => entry.name)).toEqual([name]));
    const again = await screen.findByRole("dialog", { name: "Projects with redaction off" });
    expect(await within(again).findByText(/Redaction off/)).toBeVisible();
    // Turn on has no confirm.
    await user.click(within(again).getByRole("button", { name: `Turn on redaction for ${name}` }));
    await waitFor(() => expect(view().projects_off).toEqual([]));
    expect(screen.queryByRole("dialog", { name: /^Turn off redaction/ })).toBeNull();
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
    expect(within(dialog).getByText(/Orbyn tests a new pattern on a sample of ordinary text/)).toBeVisible();
    expect(within(dialog).getByText(/Describe the shape of the value rather than pasting the value/)).toBeVisible();
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
    expect(within(table).getByText("Regular expression")).toBeVisible();
    // The list names the pattern and its type, and never carries its text.
    expect(table).not.toHaveTextContent("_[a-z]{8}");
    expect(JSON.stringify(view())).not.toContain("[a-z]{8}");
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
    expect(within(dialog).getByText(/This would hide \d+ of \d+ characters of the sample/)).toBeVisible();
    expect(within(dialog).getByText(/Hides \d+ of \d+ characters/)).toBeVisible();
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
    await waitFor(() => expect(within(dialog).getByRole("textbox", { name: "Pattern" })).toHaveValue("acme-token"));
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
    expect(within(confirm).getByText("Values that match it will no longer be hidden.")).toBeVisible();
    expect(within(confirm).getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.click(within(confirm).getByRole("button", { name: "Remove" }));
    await waitFor(() => expect(screen.queryByRole("table", { name: "Your patterns" })).toBeNull());
    expect(view().patterns).toEqual([]);
  });
});
