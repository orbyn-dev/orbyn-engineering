import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import { ApiFailure } from "../../api/client";
import { sendsPersonKey } from "../../api/personKey";
import { IssueTrackersSettings } from "./TrackerTokens";
import { setIssuesApi } from "./api";
import type { IssuesApi, TrackerRow } from "./contract";

afterEach(() => { setIssuesApi(null); vi.unstubAllGlobals(); vi.restoreAllMocks(); });
const TOKEN = "glpat-SECRET-0123456789";
const rowOf = (name: string) => screen.getByText(name, { selector: ".name" }).closest("[role=row]") as HTMLElement;
const table = () => screen.findByRole("table", { name: "Issue trackers" });
const none: TrackerRow[] = [
  { tracker: "github", connected: true },
  { tracker: "gitlab", connected: false },
  { tracker: "azure_devops", connected: false },
  { tracker: "forgejo", connected: false },
  { tracker: "linear", connected: false },
];

it("draws nothing, and asks only for the trackers, while the service answers 404", async () => {
  const fetching = vi.fn(async (_url: string) => new Response(JSON.stringify({ code: "not_found", message: "Not found." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  render(<IssueTrackersSettings />);
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
  expect(screen.queryByText("Issue trackers")).toBeNull();
  expect(fetching.mock.calls.map(([url]) => new URL(String(url), "http://x").pathname)).toEqual(["/api/issues/trackers"]);
});

it("lists the five trackers with the state and the one action that fits each", async () => {
  setIssuesApi(createIssuesMock({ trackers: [
    { tracker: "github", connected: true },
    { tracker: "gitlab", connected: true, host: "https://gitlab.com" },
    { tracker: "azure_devops", connected: false, detail: "No connection yet." },
    { tracker: "forgejo", connected: false, host: "https://git.orbyn.dev", detail: "Orbyn could not reach git.orbyn.dev. Check the address, or try again." },
    { tracker: "linear", connected: false, scope: "Orbyn team", detail: "Linear did not accept the token." },
  ] }));
  render(<IssueTrackersSettings />);
  await table();
  expect(screen.getByText("GitHub uses your gh sign-in. Tokens you add for other trackers are kept in your keychain and never shown again.")).toBeVisible();
  expect(within(rowOf("GitHub")).getByText("Uses the gh sign-in on this computer")).toBeVisible();
  expect(within(rowOf("GitHub")).getByText("Signed in")).toBeVisible();
  expect(within(rowOf("GitHub")).queryByRole("button")).toBeNull();
  expect(within(rowOf("GitLab")).getByText("Saved in your keychain · https://gitlab.com")).toBeVisible();
  expect(within(rowOf("GitLab")).getByRole("button", { name: "Replace…" })).toBeVisible();
  expect(within(rowOf("GitLab")).getByRole("button", { name: "Remove…" })).toBeVisible();
  expect(within(rowOf("Azure DevOps")).getByRole("button", { name: "Add token…" })).toBeVisible();
  expect(within(rowOf("Azure DevOps")).getByText("No connection yet.")).toBeVisible();
  expect(within(rowOf("Forgejo")).getByText("Needs attention")).toBeVisible();
  expect(within(rowOf("Forgejo")).getByRole("button", { name: "Retry" })).toBeVisible();
  expect(screen.getByText("Orbyn could not reach git.orbyn.dev. Check the address, or try again.")).toBeVisible();
  expect(within(rowOf("Linear")).getByText("Needs attention")).toBeVisible();
  // The service says why, in its own words, as text.
  expect(screen.getByText("Linear did not accept the token.")).toBeVisible();
  expect(within(rowOf("Linear")).getByRole("button", { name: "Replace…" })).toBeVisible();
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

it("keeps Save off for GitLab until a token is pasted, and for Azure DevOps until the organisation URL and the token are both filled", async () => {
  setIssuesApi(createIssuesMock({ trackers: none }));
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  let dialog = await screen.findByRole("dialog", { name: "GitLab API token" });
  expect(within(dialog).getByLabelText("GitLab address")).toHaveValue("gitlab.com");
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeDisabled();
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeEnabled();
  await user.click(within(dialog).getByRole("button", { name: "Cancel" }));
  await user.click(within(rowOf("Azure DevOps")).getByRole("button", { name: "Add token…" }));
  dialog = await screen.findByRole("dialog", { name: "Azure DevOps organisation and token" });
  expect(within(dialog).getByLabelText("Organisation URL")).toHaveValue("");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeDisabled();
  await user.type(within(dialog).getByLabelText("Organisation URL"), "dev.azure.com/orbyn");
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeEnabled();
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
  const original = mock.connect;
  mock.connect = async (tracker, input) => {
    seen = input.token ?? null;
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
  expect(mock.writes).toEqual(["connect:gitlab:https://gitlab.com"]);
  expect(seen).toBe(TOKEN);
  expect(fieldAtCall).toBe("");
  expect(within(rowOf("GitLab")).getByText("Saved")).toBeVisible();
  expect(within(rowOf("GitLab")).getByText("Saved in your keychain · https://gitlab.com")).toBeVisible();
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
  expect(mock.writes).toEqual(["connect:forgejo:https://git.orbyn.dev"]);
  expect(within(rowOf("Forgejo")).getByText("Saved in your keychain · https://git.orbyn.dev")).toBeVisible();
});

it("a tracker that rejects the token says so in the dialog, keeps nothing and asks for the token again", async () => {
  const mock = createIssuesMock({ trackers: none });
  setIssuesApi(mock);
  mock.failNext("reject");
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("The tracker did not accept the token.");
  expect(within(dialog).getByLabelText("API token")).toHaveValue("");
  expect(within(rowOf("GitLab")).getByText("Not connected")).toBeVisible();
  expect(document.body.innerHTML).not.toContain(TOKEN);
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
  const api = { ...createIssuesMock({ trackers: none }), connect: async (_: string, input: { token?: string }) => { throw new ApiFailure("bad", make(input.token ?? ""), 400); } } as unknown as IssuesApi;
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

it("sends the key with every Person-class write, and with no read", () => {
  for (const [method, path] of [
    ["POST", "/api/issues/trackers/gitlab/connect"], ["POST", "/api/issues/trackers/gitlab/disconnect"], ["POST", "/api/threads/t1/issue-links"],
    ["POST", "/api/threads/t1/issue-links/remove"], ["POST", "/api/issues/start-thread"],
  ] as const) expect(sendsPersonKey(method, path), path).toBe(true);
  for (const path of ["/api/issues/trackers", "/api/issues", "/api/issues/item?tracker=github", "/api/issues/links", "/api/issues/find?q=1", "/api/issues/draft?tracker=github"]) {
    expect(sendsPersonKey("GET", path), path).toBe(false);
  }
});

it("documents that the token goes outward once, in the body, and is never read back", () => {
  const source = readFileSync(join(process.cwd(), "src/features/issues/contract.ts"), "utf8");
  expect(source).toMatch(/JSON body and never in the path or query/);
  expect(source).toMatch(/No call reads a token back/);
});

it("Replace keeps the address in view, asks for the new token only and writes once", async () => {
  const mock = createIssuesMock({ trackers: [{ tracker: "gitlab", connected: true, host: "https://gitlab.example.com" }] });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Replace…" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByLabelText("GitLab address")).toHaveValue("https://gitlab.example.com");
  expect(within(dialog).getByText(/The saved token stays until the new one is saved/)).toBeVisible();
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["connect:gitlab:https://gitlab.example.com"]);
});

it("Remove asks first, Cancel writes nothing, Remove token is one write and the row goes back to Not connected", async () => {
  const mock = createIssuesMock({ trackers: [{ tracker: "linear", connected: true, scope: "Orbyn team" }] });
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
  expect(mock.writes).toEqual(["disconnect:linear"]);
});

it("puts the needs-attention banner above the group with an alert icon", async () => {
  setIssuesApi(createIssuesMock({ trackers: [{ tracker: "linear", connected: false, host: "https://api.linear.app", detail: "No token is saved" }] }));
  render(<IssueTrackersSettings />);
  await table();
  const section = screen.getByRole("region", { name: /Issue trackers/ });
  const alert = within(section).getByRole("alert");
  const head = within(section).getByText("Issue trackers", { selector: ".set-grp" });
  expect(alert).toHaveTextContent("No token is saved");
  expect(alert.querySelector("svg")).not.toBeNull();
  expect(alert.compareDocumentPosition(head) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  expect(within(section).getByText("Saved in your keychain · https://api.linear.app")).toBeVisible();
  expect(within(rowOf("Linear")).getByText("Needs attention")).toBeVisible();
  expect(within(rowOf("Linear")).getByRole("button", { name: "Retry" })).toBeVisible();
  expect(within(rowOf("Linear")).getByRole("button", { name: "Replace…" })).toBeVisible();
  expect(within(rowOf("Linear")).getByRole("button", { name: "Remove…" })).toBeVisible();
});

it("draws the address dialogs wide and Linear's token-only dialog narrow", async () => {
  setIssuesApi(createIssuesMock({ trackers: none }));
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  expect(await screen.findByRole("dialog", { name: "GitLab API token" })).toHaveClass("trk-dialog-address");
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
  await user.click(within(rowOf("Azure DevOps")).getByRole("button", { name: "Add token…" }));
  expect(await screen.findByRole("dialog", { name: "Azure DevOps organisation and token" })).toHaveClass("trk-dialog-address");
  await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Add token…" }));
  const linear = await screen.findByRole("dialog", { name: "Linear API token" });
  expect(linear).not.toHaveClass("trk-dialog-address");
  await user.type(within(linear).getByLabelText("API token"), TOKEN);
  expect(within(linear).getByRole("button", { name: "Save token" })).toBeEnabled();
});

it("saves Linear's token with its one fixed host, and the row says that host", async () => {
  const mock = createIssuesMock({ trackers: none });
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Linear")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog", { name: "Linear API token" });
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  expect(mock.writes).toEqual(["connect:linear:"]);
  expect(within(rowOf("Linear")).getByText("Saved in your keychain · https://api.linear.app")).toBeVisible();
});

it("a keychain that cannot keep the token says so under the fields, keeps the address and saves nothing", async () => {
  const mock = createIssuesMock({ trackers: none });
  mock.failNext("keychain");
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Forgejo")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog", { name: "Forgejo address and token" });
  await user.type(within(dialog).getByLabelText("Forgejo address"), "git.orbyn.dev");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("There is no usable keychain here, so a token cannot be saved.");
  expect(within(dialog).getByLabelText("Forgejo address")).toHaveValue("git.orbyn.dev");
  expect(within(dialog).getByLabelText("API token")).toHaveValue("");
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeDisabled();
  expect(within(rowOf("Forgejo")).getByText("Not connected")).toBeVisible();
  expect(document.body.innerHTML).not.toContain(TOKEN);
});

it("a refused address says the service's words, keeps the address and asks for the token again", async () => {
  const mock = createIssuesMock({ trackers: none });
  mock.failNext("host");
  setIssuesApi(mock);
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("GitLab")).getByRole("button", { name: "Add token…" }));
  const dialog = await screen.findByRole("dialog", { name: "GitLab API token" });
  const address = within(dialog).getByLabelText("GitLab address");
  await user.clear(address);
  await user.type(address, "http://gitlab.example.com");
  await user.type(within(dialog).getByLabelText("API token"), TOKEN);
  await user.click(within(dialog).getByRole("button", { name: "Save token" }));
  expect(await within(dialog).findByRole("alert")).toHaveTextContent("That host is not allowed: it must start with https://.");
  expect(address).toHaveValue("http://gitlab.example.com");
  expect(within(dialog).getByLabelText("API token")).toHaveValue("");
  expect(within(dialog).getByRole("button", { name: "Save token" })).toBeDisabled();
  expect(within(rowOf("GitLab")).getByText("Not connected")).toBeVisible();
  expect(document.body.innerHTML).not.toContain(TOKEN);
});

it("Remove on a stored Forgejo connection asks first with Cancel focused and writes nothing", async () => {
  setIssuesApi(createIssuesMock({ trackers: [{ tracker: "forgejo", connected: true, host: "https://git.orbyn.dev" }] }));
  const user = userEvent.setup();
  render(<IssueTrackersSettings />);
  await table();
  await user.click(within(rowOf("Forgejo")).getByRole("button", { name: "Remove…" }));
  const dialog = await screen.findByRole("dialog", { name: "Remove Forgejo’s token from the keychain?" });
  expect(within(dialog).getByText("Orbyn deletes the token it saved. Issues from Forgejo stop showing until you add one again. Linked threads keep their links.")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
});

it("lets the status chip take its own width and stacks the row actions when the table is narrow", () => {
  const source = readFileSync(join(process.cwd(), "src/features/issues/trackers.css"), "utf8");
  expect(source).toMatch(/\.trk-table \.trk-row \{[^}]*grid-template-columns: 28px minmax\(0, 1fr\) auto auto/);
  expect(source).toMatch(/\.trk-table \{[^}]*container-type: inline-size/);
  expect(source).toMatch(/@container \(max-width: 560px\)/);
  expect(source).toMatch(/\.trk-table \.trk-row \.trk-acts \{[^}]*grid-column: 2 \/ -1/);
});

it("Retry on a connection that is not working reads the trackers again and writes nothing", async () => {
  const mock = createIssuesMock({ trackers: [{ tracker: "forgejo", connected: false, host: "https://git.orbyn.dev", detail: "No answer from that host." }] });
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
