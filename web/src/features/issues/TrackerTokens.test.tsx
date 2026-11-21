import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import { ApiFailure } from "../../api/client";
import { IssueTrackersSettings } from "./TrackerTokens";
import { setIssuesApi } from "./api";
import type { IssuesApi, TrackerRow } from "./contract";

afterEach(() => { setIssuesApi(null); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const TOKEN = "glpat-SECRET-0123456789";
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;
const table = () => screen.findByRole("table", { name: "Issue trackers" });
const none: TrackerRow[] = [
  { tracker: "github", connected: true, status: "signed_in", account: "sam-arden" },
  { tracker: "gitlab", connected: false, status: "not_connected" },
  { tracker: "azure_devops", connected: false, status: "not_connected" },
  { tracker: "forgejo", connected: false, status: "not_connected" },
  { tracker: "linear", connected: false, status: "not_connected" },
];

it("draws nothing, and sends no request, while the service reports no trackers", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  render(<IssueTrackersSettings />);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByText("Issue trackers")).toBeNull();
  expect(fetching).not.toHaveBeenCalled();
});

it("lists the five trackers with the state and the one action that fits each", async () => {
  setIssuesApi(createIssuesMock({ trackers: [
    { tracker: "github", connected: true, status: "signed_in", account: "sam-arden" },
    { tracker: "gitlab", connected: true, status: "saved", address: "gitlab.com" },
    { tracker: "azure_devops", connected: false, status: "not_connected" },
    { tracker: "forgejo", connected: true, status: "unreachable", address: "git.orbyn.dev" },
    { tracker: "linear", connected: false, status: "rejected", scope: "Orbyn team" },
  ] }));
  render(<IssueTrackersSettings />);
  await table();
  expect(screen.getByText("GitHub uses your gh sign-in. Tokens you add for other trackers are kept in your keychain and never shown again.")).toBeVisible();
  expect(within(rowOf("GitHub")).getByText("Uses the gh sign-in on this computer · sam-arden")).toBeVisible();
  expect(within(rowOf("GitHub")).getByText("Signed in")).toBeVisible();
  expect(within(rowOf("GitHub")).queryByRole("button")).toBeNull();
  expect(within(rowOf("GitLab")).getByText("Saved in your keychain · gitlab.com")).toBeVisible();
  expect(within(rowOf("GitLab")).getByRole("button", { name: "Replace…" })).toBeVisible();
  expect(within(rowOf("GitLab")).getByRole("button", { name: "Remove…" })).toBeVisible();
  expect(within(rowOf("Azure DevOps")).getByRole("button", { name: "Add token…" })).toBeVisible();
  expect(within(rowOf("Forgejo")).getByText("Can’t reach")).toBeVisible();
  expect(within(rowOf("Forgejo")).getByRole("button", { name: "Retry" })).toBeVisible();
  expect(screen.getByText("Orbyn could not reach git.orbyn.dev. Check the address, or try again.")).toBeVisible();
  expect(within(rowOf("Linear")).getByText("Rejected")).toBeVisible();
  expect(screen.getByText("Linear didn’t accept the saved token. Replace it or remove it.")).toBeVisible();
});

it("asks for the address each tracker needs, and Linear and GitHub for none", async () => {
  setIssuesApi(createIssuesMock({ trackers: none }));
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  let dialog = await screen.findByRole("dialog", { name: "GitLab API token" });
  expect(within(dialog).getByLabelText("GitLab address")).toHaveValue("gitlab.com");
  expect(within(dialog).getByText("Leave gitlab.com, or enter your own server, for example gitlab.example.com.")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await user.click(within(rowOf("Forgejo")).getByRole("button", { name: "Add token…" }));
  dialog = await screen.findByRole("dialog", { name: "Forgejo address and token" });
  expect(within(dialog).getByLabelText("Forgejo address")).toHaveValue("");
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await user.click(within(rowOf("Azure DevOps")).getByRole("button", { name: "Add token…" }));
  dialog = await screen.findByRole("dialog", { name: "Azure DevOps organisation and token" });
  expect(within(dialog).getByLabelText("Organisation URL")).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Add token…" }));
  dialog = await screen.findByRole("dialog", { name: "Linear API token" });
  expect(within(dialog).queryByLabelText(/address|URL/i)).toBeNull();
});

it("keeps the token in a hidden, uncontrolled field that password managers are told to leave alone", async () => {
  setIssuesApi(createIssuesMock({ trackers: none }));
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Add token…" }));
  const field = within(await screen.findByRole("dialog")).getByLabelText("API token");
  expect(field).toHaveAttribute("type", "password");
  expect(field).toHaveAttribute("autocomplete", "new-password");
  expect(field).toHaveAttribute("data-1p-ignore", "true");
  expect(field).toHaveAttribute("data-lpignore", "true");
  expect(field).toHaveAttribute("data-bwignore", "true");
  await user.type(field, TOKEN);
  // React puts no value attribute in the page: what is typed lives in the field only.
  expect(field.getAttribute("value")).toBeNull();
  expect(screen.getByText("Hidden while you type. Never written to a file or a log.")).toBeVisible();
});

it("Save is one write however fast the clicks, the field is empty by then, and the row says Saved with no token anywhere", async () => {
  const mock = createIssuesMock({ trackers: none, delayMs: 30 });
  let seen: string | null = "unset";
  let fieldAtCall: string | null = "unset";
  const original = mock.saveToken;
  mock.saveToken = async (tracker, input) => {
    seen = input.token;
    fieldAtCall = document.querySelector<HTMLInputElement>("input[type=password]")?.value ?? null;
    return original(tracker, input);
  };
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  const save = within(dialog).getByRole("button", { name: "Save token" });
  await user.click(save);
  await user.click(save);
  await user.click(save);
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["token:gitlab:gitlab.com"]);
  expect(seen).toBe(TOKEN);
  expect(fieldAtCall).toBe("");
  expect(within(rowOf("GitLab")).getByText("Saved")).toBeVisible();
  expect(within(rowOf("GitLab")).getByText("Saved in your keychain · gitlab.com")).toBeVisible();
  expect(document.body.innerHTML).not.toContain(TOKEN);
  expect(JSON.stringify(mock.writes)).not.toContain(TOKEN);
  expect(JSON.stringify({ ...localStorage })).not.toContain(TOKEN);
  expect(window.location.href).not.toContain(TOKEN);
});

it("Enter in the field is the same one save, and Save stays off until the token and the address are filled", async () => {
  const mock = createIssuesMock({ trackers: none });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Forgejo")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog");
  const save = within(dialog).getByRole("button", { name: "Save token" });
  expect(save).toBeDisabled();
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  expect(save).toBeDisabled();
  await user.type(within(dialog).getByLabelText("Forgejo address"), "git.orbyn.dev");
  expect(save).toBeEnabled();
  await user.type(within(dialog).getByLabelText("API token"), "{Enter}");
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["token:forgejo:git.orbyn.dev"]);
  expect(within(rowOf("Forgejo")).getByText("Saved in your keychain · git.orbyn.dev")).toBeVisible();
});

it("a tracker that rejects the token shows Rejected, with Replace and Remove", async () => {
  const mock = createIssuesMock({ trackers: none });
  setIssuesApi(mock);
  mock.failNext("reject");
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  await user.type(within(await screen.findByRole("dialog")).getByLabelText("API token"), TOKEN);
  await user.click(screen.getByRole("button", { name: "Save token" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(within(rowOf("GitLab")).getByText("Rejected")).toBeVisible();
  expect(screen.getByText("GitLab didn’t accept the saved token. Replace it or remove it.")).toBeVisible();
  expect(within(rowOf("GitLab")).getByRole("button", { name: "Replace…" })).toBeVisible();
  expect(within(rowOf("GitLab")).getByRole("button", { name: "Remove…" })).toBeVisible();
});

it("an address that does not answer says so in the dialog, saves nothing and asks for the token again", async () => {
  const mock = createIssuesMock({ trackers: none });
  setIssuesApi(mock);
  mock.failNext("unreachable");
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Forgejo")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByLabelText("Forgejo address"), "git.orbyn.dev");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("Orbyn could not reach git.orbyn.dev. Check the address, or try again.");
  expect(within(dialog).getByLabelText("API token")).toHaveValue("");
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeDisabled();
  expect(within(rowOf("Forgejo")).getByText("Not connected")).toBeVisible();
  expect(document.body.innerHTML).not.toContain(TOKEN);
});

const echoes = (make: (token: string) => string) => async () => {
  const api = { ...createIssuesMock({ trackers: none }), saveToken: async (_: string, input: { token: string }) => { throw new ApiFailure("bad", make(input.token), 400); } } as unknown as IssuesApi;
  setIssuesApi(api);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("The tracker refused the token.");
  expect(document.body.innerHTML).not.toContain(TOKEN);
  expect(document.body.textContent).not.toContain(btoa(`:${TOKEN}`));
};
const B64 = (text: string) => btoa(text);
it("never shows a token the service echoes back as typed", echoes((token) => `Bad token ${token} for you`));
it("never shows it in another case", echoes((token) => `Bad ${token.toUpperCase()} here`));
it("never shows it URL-encoded", echoes((token) => `GET /x?t=${encodeURIComponent(token)}`));
it("never shows it JSON-escaped", echoes((token) => JSON.stringify({ token }).slice(1, -1)));
it("never shows it as base64", echoes((token) => `Authorization ${B64(token)}`));
it("never shows it as Basic base64 of :token (Azure DevOps)", echoes((token) => `Basic ${B64(`:${token}`)}`));
it("never shows a token that sits past the 300 character cut", echoes((token) => `${"x".repeat(310)} ${token}`));

it("documents that the real issue routes are Person-class, carry the token in the body and never echo it", () => {
  const source = readFileSync(join(process.cwd(), "src/features/issues/contract.ts"), "utf8");
  for (const operation of ["saveToken", "removeToken", "setState", "link", "unlink", "startThread"]) {
    const at = source.indexOf(`  ${operation}(`);
    expect(at, operation).toBeGreaterThan(0);
    expect(source.slice(Math.max(0, at - 900), at), operation).toContain("Person-class (ENFORCED");
  }
  expect(source).toMatch(/JSON body and never in the path or query/);
  expect(source).toMatch(/must not\s+\* echo the token/);
});

it("Replace keeps the address in view, asks for the new token only and writes once", async () => {
  const mock = createIssuesMock({ trackers: [{ tracker: "gitlab", connected: true, status: "saved", address: "gitlab.example.com" }] });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Replace…" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByLabelText("GitLab address")).toHaveValue("gitlab.example.com");
  expect(within(dialog).getByText(/The saved token stays until the new one is saved/)).toBeVisible();
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["token:gitlab:gitlab.example.com"]);
});

it("Remove asks first, Cancel writes nothing, Remove token is one write and the row goes back to Not connected", async () => {
  const mock = createIssuesMock({ trackers: [{ tracker: "linear", connected: true, status: "saved", scope: "Orbyn team" }] });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Remove…" }));
  let dialog = await screen.findByRole("dialog", { name: "Remove Linear’s token from the keychain?" });
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual([]);
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Remove…" }));
  dialog = await screen.findByRole("dialog");
  await user.dblClick(within(dialog).getByRole("button", { name: "Remove token" }));
  await waitFor(() => expect(within(rowOf("Linear")).getByText("Not connected")).toBeVisible());
  expect(mock.writes).toEqual(["remove-token:linear"]);
});

it("Retry on a host that did not answer reads the trackers again and writes nothing", async () => {
  const mock = createIssuesMock({ trackers: [{ tracker: "forgejo", connected: true, status: "unreachable", address: "git.orbyn.dev" }] });
  const reads = vi.spyOn(mock, "trackers");
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  const before = reads.mock.calls.length;
  await user.click(within(rowOf("Forgejo")).getByRole("button", { name: "Retry" }));
  await waitFor(() => expect(reads.mock.calls.length).toBeGreaterThan(before));
  expect(mock.writes).toEqual([]);
});
