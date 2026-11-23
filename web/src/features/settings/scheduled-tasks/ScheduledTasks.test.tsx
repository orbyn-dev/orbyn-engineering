import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import * as fixtures from "../../../../mock/fixtures";
import { api } from "../../../api/client";
import type { Project } from "../../../api/types";
import { useWorkspace } from "../../../product/state/workspace";
import { installMockFetch } from "../../../test-support";
import { ScheduledTasksSection } from "./ScheduledTasks";

afterEach(() => { vi.restoreAllMocks(); });

function ready() {
  useWorkspace.setState({
    projects: { data: fixtures.projects, error: null, loading: false },
    engines: { data: fixtures.engines, error: null, loading: false },
  });
}

it("opens the linked task's page on its Runs tab, and the editor only from Edit", async () => {
  ready();
  window.location.hash = "#/settings/tasks?task=task-review";
  installMockFetch();
  const user = userEvent.setup();
  render(<ScheduledTasksSection />);

  expect(await screen.findByRole("heading", { name: "Daily review" })).toBeVisible();
  expect(screen.getByRole("tab", { name: "Runs", selected: true })).toBeVisible();
  expect(screen.queryByRole("dialog", { name: "Edit scheduled task" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Edit" }));
  expect(await screen.findByRole("dialog", { name: "Edit scheduled task" })).toBeVisible();
});

it("opens the editor straight away from an Edit link", async () => {
  ready();
  window.location.hash = "#/settings/tasks?task=task-review&edit=1";
  installMockFetch();
  render(<ScheduledTasksSection />);
  expect(await screen.findByRole("dialog", { name: "Edit scheduled task" })).toBeVisible();
});

it("drops a link to a task that no longer exists without opening an editor", async () => {
  ready();
  window.location.hash = "#/settings/tasks?task=gone";
  installMockFetch();
  render(<ScheduledTasksSection />);

  await waitFor(() => expect(window.location.hash).toBe("#/settings/tasks"));
  expect(screen.queryByRole("dialog", { name: "Edit scheduled task" })).toBeNull();
});

it("deletes a task only after the second question inside Edit", async () => {
  ready();
  window.location.hash = "#/settings/tasks";
  installMockFetch();
  const calls = { some: (test: (call: { method: string; path: string }) => boolean) => vi.mocked(fetch).mock.calls.map(([url, init]) => ({ method: (init as RequestInit | undefined)?.method ?? "GET", path: String(url) })).some(test) };
  const user = userEvent.setup();
  render(<ScheduledTasksSection />);
  await user.click(await screen.findByRole("button", { name: "Edit Daily review" }));
  await user.click(await screen.findByRole("button", { name: "Delete task…" }));
  expect(calls.some((call) => call.method === "DELETE")).toBe(false);
  const ask = await screen.findByRole("dialog", { name: "Delete Daily review?" });
  await user.click(within(ask).getByRole("button", { name: "Delete task" }));
  await waitFor(() => expect(calls.some((call) => call.method === "DELETE" && call.path.includes("task-review"))).toBe(true));
});

it("opens a task from its row and starts a run from there", async () => {
  ready();
  window.location.hash = "#/settings/tasks";
  installMockFetch();
  const calls = { some: (test: (call: { method: string; path: string }) => boolean) => vi.mocked(fetch).mock.calls.map(([url, init]) => ({ method: (init as RequestInit | undefined)?.method ?? "GET", path: String(url) })).some(test) };
  const user = userEvent.setup();
  render(<ScheduledTasksSection />);
  await user.click(await screen.findByRole("button", { name: "Open Daily review" }));
  expect(await screen.findByRole("heading", { name: "Daily review" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Run now" }));
  await waitFor(() => expect(calls.some((call) => call.method === "POST" && call.path.endsWith("/task-review/run"))).toBe(true));
});

it("puts Name first, typed by the person, and refuses to save without one", async () => {
  ready();
  window.location.hash = "#/settings/tasks?task=task-review";
  installMockFetch();
  const user = userEvent.setup();
  render(<ScheduledTasksSection />);
  await user.click(await screen.findByRole("button", { name: "Edit" }));
  const dialog = await screen.findByRole("dialog", { name: "Edit scheduled task" });
  const name = within(dialog).getByRole("textbox", { name: "Name" });
  expect(dialog.querySelector("input, textarea, select")).toBe(name);
  expect(within(dialog).queryByText("Give the task a name.")).toBeNull();
  await user.clear(name);
  expect(within(dialog).getByText("Give the task a name.")).toBeVisible();
  expect(name).toHaveAttribute("aria-invalid", "true");
  expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeDisabled();
  await user.type(name, "Morning review");
  expect(within(dialog).queryByText("Give the task a name.")).toBeNull();
  expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeEnabled();
});

async function openIncomingHook() {
  ready();
  window.location.hash = "#/settings/tasks";
  installMockFetch();
  const user = userEvent.setup();
  render(<ScheduledTasksSection />);
  await user.click(await screen.findByText("Incoming hook"));
  expect(await screen.findByRole("heading", { name: "Incoming hook" })).toBeVisible();
  return user;
}

it("opens the task page from a row click, with Edit and Run now in its header and Runs first among its tabs", async () => {
  const user = await openIncomingHook();
  expect(screen.queryByRole("button", { name: "History" })).toBeNull();
  expect(screen.getByRole("button", { name: "Run now" })).toBeVisible();
  expect(screen.getByText("Handle the webhook.")).toBeVisible();
  expect(screen.getByText("Webhook", { selector: "p" })).toBeVisible();
  expect(screen.getAllByRole("tab").map((tab) => tab.textContent)).toEqual(["Runs", "Webhook deliveries"]);
  await user.click(screen.getByRole("button", { name: "Edit" }));
  expect(await screen.findByRole("dialog", { name: "Edit scheduled task" })).toBeVisible();
});

it("names each of the eight outcomes and gives a refused or skipped run its reason", async () => {
  await openIncomingHook();
  const runs = await screen.findByRole("table", { name: "Runs" });
  const results = within(runs).getAllByRole("row").slice(1).map((row) => within(row).getAllByRole("cell")[2]?.textContent);
  expect(results).toEqual(["Queued", "Running", "Completed", "Failed", "Cancelled", "Interrupted", "Refused · webhook answer 401", "Skipped · limit window 92% used"]);
  expect(screen.getByText("Last 50 runs")).toBeVisible();
});

it("draws Interrupted, Refused and Skipped as neutral marks: only Failed is red and Running blue", async () => {
  await openIncomingHook();
  const runs = await screen.findByRole("table", { name: "Runs" });
  const pill = (label: string) => within(runs).getByText(label).closest(".set-pill");
  for (const label of ["Interrupted", "Refused", "Skipped", "Cancelled", "Queued"]) expect(pill(label)).toHaveProperty("className", "set-pill");
  expect(pill("Failed")).toHaveClass("bad");
  expect(pill("Running")).toHaveClass("blue");
});

it("lists each task with when it runs and nothing under it but the schedule", async () => {
  ready();
  window.location.hash = "#/settings/tasks";
  installMockFetch();
  render(<ScheduledTasksSection />);
  const row = (await screen.findByRole("button", { name: "Open Daily review" })).closest("[role=row]") as HTMLElement;
  expect(within(row).getAllByRole("cell")[1]).toHaveTextContent(/^Every 30 min$/);
});

it("shows who sent each delivery as the sender's own claim, cut at 80 characters, and nothing when it names none", async () => {
  const user = await openIncomingHook();
  await user.click(screen.getByRole("tab", { name: "Webhook deliveries" }));
  const table = await screen.findByRole("table", { name: "Deliveries" });
  expect(within(table).getByRole("columnheader", { name: "Sent by" })).toBeVisible();
  expect(within(table).getByText("From GitHub")).toHaveAttribute("title", "As reported by the sender");
  const long = within(table).getByText(/^From Mozilla/);
  expect(long.textContent).toBe(`From ${"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36".slice(0, 80)}…`);
  const rows = within(table).getAllByRole("row");
  expect(within(rows[4] as HTMLElement).getAllByRole("cell")[1]).toBeEmptyDOMElement();
  expect(screen.queryByText("From", { exact: true })).toBeNull();
});

it("answers a delivery as 202 Accepted or 200 with one of six outcomes, each explained, and only Failed in the failed style", async () => {
  const delivery = (id: string, status: string) => ({ id, task_id: "task-hook", status, received_at: "2026-10-30T10:00:00Z", headers: {}, body: "{}", prompt: null, thread_id: null });
  vi.spyOn(api, "taskDeliveries").mockResolvedValue(["accepted", "bad_signature", "filtered", "duplicate", "stale", "disabled", "failed"].map((status, at) => delivery(`d${at}`, status)));
  const user = await openIncomingHook();
  await user.click(screen.getByRole("tab", { name: "Webhook deliveries" }));
  const table = await screen.findByRole("table", { name: "Deliveries" });
  const answers = ["202 Accepted", "200 Bad signature", "200 Filtered out", "200 Duplicate", "200 Stale", "200 Disabled", "200 Failed"];
  expect(answers.map((text) => within(table).getByText(text).textContent)).toEqual(answers);
  expect(within(table).getByText("200 Bad signature")).toHaveAttribute("title", "The signature didn’t match. Nothing ran.");
  expect(within(table).getByText("200 Stale")).toHaveAttribute("title", "Too old to run. Nothing ran.");
  expect(within(table).getByText("200 Failed")).toHaveAttribute("title", "Accepted, but the run failed.");
  expect(within(table).getByText("200 Failed")).toHaveClass("bad");
  expect(within(table).getByText("200 Bad signature")).not.toHaveClass("bad");
});

it("with no address yet, the deliveries tab says there are none and how to get some", async () => {
  vi.spyOn(api, "taskWebhook").mockResolvedValue({ configured: false, path_prefix: "/hooks/task-hook/" });
  vi.spyOn(api, "taskDeliveries").mockResolvedValue([]);
  const user = await openIncomingHook();
  await user.click(screen.getByRole("tab", { name: "Webhook deliveries" }));
  expect(await screen.findByText("No deliveries yet.")).toBeVisible();
  expect(await screen.findByText("Create an address to receive them.")).toBeVisible();
});

it("Run now reloads the Runs tab as well as the task list", async () => {
  const runs = vi.spyOn(api, "taskRuns").mockResolvedValue([]);
  vi.spyOn(api, "runTask").mockResolvedValue(undefined as never);
  const user = await openIncomingHook();
  await waitFor(() => expect(runs).toHaveBeenCalledTimes(1));
  await user.click(screen.getByRole("button", { name: "Run now" }));
  await waitFor(() => expect(runs).toHaveBeenCalledTimes(2));
});

it("shows the stored webhook address as configured, and the full address with Copy only right after it is regenerated", async () => {
  const user = await openIncomingHook();
  expect(await screen.findByText("Configured · /hooks/task-hook/…")).toBeVisible();
  expect(screen.getByText("Anyone with this address can start the task. Keep it private.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Copy" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Regenerate address" }));
  expect(await screen.findByRole("button", { name: "Copy" })).toBeVisible();
  expect(screen.getByText("Copy it now. It isn’t shown again.")).toBeVisible();
  expect(screen.getByText(/\/hooks\/task-hook\/tok\d+x9f$/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Regenerate address" })).toBeNull();
});

it("offers Create address while a webhook task has none, and Regenerate address once it does", async () => {
  vi.spyOn(api, "taskWebhook").mockResolvedValue({ configured: false, path_prefix: "/hooks/task-hook/" });
  const user = await openIncomingHook();
  expect(await screen.findByText("No webhook address yet.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Regenerate address" })).toBeNull();
  vi.mocked(api.taskWebhook).mockResolvedValue({ configured: true, path_prefix: "/hooks/task-hook/" });
  await user.click(screen.getByRole("button", { name: "Create address" }));
  expect(await screen.findByRole("button", { name: "Copy" })).toBeVisible();
  expect(screen.getByText("Copy it now. It isn’t shown again.")).toBeVisible();
});

it("says so when a task has no runs yet", async () => {
  vi.spyOn(api, "taskRuns").mockResolvedValue([]);
  await openIncomingHook();
  expect(await screen.findByText("No runs yet.")).toBeVisible();
  expect(screen.getByText("Runs appear here once the task has run.")).toBeVisible();
});

it("shows the two task defaults on the settings page, with Run while closed gone", async () => {
  ready();
  window.location.hash = "#/settings/tasks";
  installMockFetch();
  render(<ScheduledTasksSection />);
  expect(await screen.findByRole("switch", { name: "Skip a run when a limit is near" })).toBeVisible();
  expect(screen.getByText("Waits until Codex’s limit window has room (90% used or more). Engines that report no window are never held. Applies to scheduled and follow-up runs.")).toBeVisible();
  expect(screen.getByRole("switch", { name: "Notify when a run finishes or fails" })).toBeChecked();
  expect(screen.queryByText(/while closed/i)).toBeNull();
});

/** Opens New scheduled task from the task list the mock serves. */
async function openNewTask() {
  window.location.hash = "#/settings/tasks";
  const user = userEvent.setup();
  render(<ScheduledTasksSection />);
  // The list settles from its loading state into rows or the empty state; a click on the button
  // that settle replaces would be lost.
  await waitFor(() => expect(screen.queryByText("Loading tasks")).toBeNull());
  await user.click(await screen.findByRole("button", { name: "New task" }));
  const dialog = await screen.findByRole("dialog", { name: "New scheduled task" });
  return { user, dialog };
}

it("clamps an interval below one minute on blur and when saving without blur", async () => {
  ready();
  installMockFetch();
  const { user, dialog } = await openNewTask();
  await user.type(within(dialog).getByRole("textbox", { name: "Name" }), "Check");
  await user.type(within(dialog).getByRole("textbox", { name: "What should the agent do?" }), "Review");
  const interval = within(dialog).getByRole("spinbutton", { name: "Every (minutes)" });
  for (const value of ["-3", "0", ""]) {
    fireEvent.change(interval, { target: { value } });
    fireEvent.blur(interval);
    expect(interval).toHaveValue(1);
  }
  fireEvent.change(interval, { target: { value: "12" } });
  fireEvent.blur(interval);
  expect(interval).toHaveValue(12);
  fireEvent.change(interval, { target: { value: "-3" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Save task" }));
  await waitFor(() => {
    const call = vi.mocked(fetch).mock.calls.find(([url, init]) => String(url).endsWith("/api/tasks") && init?.method === "POST");
    expect(call).toBeDefined();
    const body = JSON.parse(String(call![1]?.body));
    expect(body.schedule.every_minutes).toBe(1);
  });
});

it("restores the last valid interval when editing clears or enters a fractional value", async () => {
  ready();
  window.location.hash = "#/settings/tasks?task=task-review&edit=1";
  installMockFetch();
  render(<ScheduledTasksSection />);
  const dialog = await screen.findByRole("dialog", { name: "Edit scheduled task" });
  const interval = within(dialog).getByRole("spinbutton", { name: "Every (minutes)" });
  expect(interval).toHaveValue(30);
  fireEvent.change(interval, { target: { value: "" } });
  fireEvent.blur(interval);
  expect(interval).toHaveValue(30);
  fireEvent.change(interval, { target: { value: "15" } });
  fireEvent.change(interval, { target: { value: "1.5" } });
  fireEvent.blur(interval);
  expect(interval).toHaveValue(15);
  fireEvent.change(interval, { target: { value: "4294967296" } });
  fireEvent.blur(interval);
  expect(interval).toHaveValue(15);
  fireEvent.change(interval, { target: { value: "" } });
  fireEvent.click(within(dialog).getByRole("button", { name: "Save changes" }));
  await waitFor(() => {
    const call = vi.mocked(fetch).mock.calls.find(([url, init]) => String(url).endsWith("/api/tasks/task-review") && init?.method === "PUT");
    expect(call).toBeDefined();
    const body = JSON.parse(String(call![1]?.body));
    expect(body.schedule.every_minutes).toBe(15);
  });
});

it.each([["", 30], ["1.5", 30], ["4294967296", 30]])(
  "saves a safe interval for %s without relying on blur",
  async (value, expected) => {
    ready();
    window.location.hash = "#/settings/tasks?task=task-review&edit=1";
    installMockFetch();
    render(<ScheduledTasksSection />);
    const dialog = await screen.findByRole("dialog", { name: "Edit scheduled task" });
    fireEvent.change(within(dialog).getByRole("spinbutton", { name: "Every (minutes)" }), { target: { value } });
    fireEvent.click(within(dialog).getByRole("button", { name: "Save changes" }));
    await waitFor(() => {
      const call = vi.mocked(fetch).mock.calls.find(([url, init]) => String(url).endsWith("/api/tasks/task-review") && init?.method === "PUT");
      expect(call).toBeDefined();
      expect(JSON.parse(String(call![1]?.body)).schedule.every_minutes).toBe(expected);
    });
  },
);

it("offers the projects the rest of the app lists, choosing the first", async () => {
  installMockFetch();
  await useWorkspace.getState().refreshAll();
  const { dialog } = await openNewTask();
  const picker = within(dialog).getByRole("combobox", { name: "Project" });
  expect(within(picker).getAllByRole("option").map((option) => option.textContent)).toEqual(
    fixtures.projects.map((project) => project.name),
  );
  expect(picker).toHaveValue("p-app");
});

it("leaves the skills library and scratch projects out of the Project list, as every other list does", async () => {
  const library: Project = { ...fixtures.projects[0]!, id: "p-library", name: "Skills library", library: true };
  const scratch: Project = { ...fixtures.projects[1]!, id: "p-scratch", name: "Scratch", scratch: true };
  useWorkspace.setState({
    projects: { data: [...fixtures.projects, library, scratch], error: null, loading: false },
    engines: { data: fixtures.engines, error: null, loading: false },
  });
  installMockFetch();
  const { dialog } = await openNewTask();
  const picker = within(dialog).getByRole("combobox", { name: "Project" });
  expect(within(picker).getAllByRole("option").map((option) => option.textContent)).toEqual(
    fixtures.projects.map((project) => project.name),
  );
});

it("keeps an edited task's own project, and Save, even when that project is not in the list", async () => {
  const hidden: Project = { ...fixtures.projects[0]!, id: "p-app", name: "Skills library", library: true };
  useWorkspace.setState({
    projects: { data: [hidden, ...fixtures.projects.slice(1).filter((entry) => entry.id !== "p-app")], error: null, loading: false },
    engines: { data: fixtures.engines, error: null, loading: false },
  });
  window.location.hash = "#/settings/tasks?task=task-review&edit=1";
  installMockFetch();
  render(<ScheduledTasksSection />);
  const dialog = await screen.findByRole("dialog", { name: "Edit scheduled task" });
  expect(within(dialog).getByRole("button", { name: "Save changes" })).toBeEnabled();
  expect(within(dialog).getByRole("combobox", { name: "Project" })).toHaveValue("p-app");
});

it("uses the projects when the workspace read answers after the dialog opens, and says it is still reading until then", async () => {
  useWorkspace.setState({
    projects: { data: null, error: null, loading: true },
    engines: { data: fixtures.engines, error: null, loading: false },
  });
  installMockFetch();
  const { dialog } = await openNewTask();
  const picker = within(dialog).getByRole("combobox", { name: "Project" });
  expect(within(picker).getAllByRole("option").map((option) => option.textContent)).toEqual(["Loading projects…"]);
  const save = within(dialog).getByRole("button", { name: "Save task" });
  expect(save).toBeDisabled();
  expect(save).toHaveAccessibleDescription("Save task needs a name, what the agent should do and a project.");

  act(() => useWorkspace.setState({ projects: { data: fixtures.projects, error: null, loading: false } }));
  await waitFor(() => expect(picker).toHaveValue("p-app"));
  expect(within(picker).getAllByRole("option").map((option) => option.textContent)).toEqual(
    fixtures.projects.map((project) => project.name),
  );
});

it("with no projects, says to add one first and keeps Save's reason visible and accessible", async () => {
  const mock = installMockFetch();
  mock.reset("empty");
  await useWorkspace.getState().refreshAll();
  const { user, dialog } = await openNewTask();
  const picker = within(dialog).getByRole("combobox", { name: "Project" });
  expect(picker).toHaveAttribute("required");
  expect(within(picker).getAllByRole("option").map((option) => option.textContent)).toEqual(["No projects yet"]);
  expect(picker).toHaveAccessibleDescription("No projects yet. Add one in Settings › Projects, then come back.");
  const save = within(dialog).getByRole("button", { name: "Save task" });
  expect(save).toBeDisabled();
  expect(save).toHaveAccessibleDescription("Save task needs a name, what the agent should do and a project.");

  await user.type(within(dialog).getByRole("textbox", { name: "Name" }), "Nightly triage");
  await user.type(within(dialog).getByRole("textbox", { name: "What should the agent do?" }), "Triage new issues.");
  expect(save).toBeDisabled();
  expect(save).toHaveAccessibleDescription("Save task needs a project.");
});

it("names what Save still needs instead of disabling silently", async () => {
  ready();
  installMockFetch();
  const { user, dialog } = await openNewTask();
  const save = within(dialog).getByRole("button", { name: "Save task" });
  const reason = "Save task needs a name and what the agent should do.";
  expect(save).toBeDisabled();
  expect(save).toHaveAccessibleDescription(reason);
  expect(within(dialog).getByText(reason)).toBeVisible();

  await user.type(within(dialog).getByRole("textbox", { name: "What should the agent do?" }), "Triage new issues.");
  expect(save).toHaveAccessibleDescription("Save task needs a name.");

  await user.type(within(dialog).getByRole("textbox", { name: "Name" }), "Nightly triage");
  expect(save).toBeEnabled();
  expect(save).not.toHaveAccessibleDescription();
  expect(within(dialog).queryByText(/^Save task needs/)).toBeNull();
});

it("shows New task once while the list is empty (the empty state's) and in the header once tasks exist", async () => {
  ready();
  window.location.hash = "#/settings/tasks";
  installMockFetch();
  const mockFetch = globalThis.fetch;
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) =>
    input === "/api/tasks" && (init?.method ?? "GET") === "GET"
      ? new Response("[]", { status: 200, headers: { "content-type": "application/json" } })
      : mockFetch(input, init)));
  const empty = render(<ScheduledTasksSection />);
  expect(await screen.findByText("No scheduled tasks")).toBeVisible();
  expect(screen.getAllByRole("button", { name: "New task" })).toHaveLength(1);
  empty.unmount();
  vi.stubGlobal("fetch", mockFetch);
  render(<ScheduledTasksSection />);
  await screen.findByRole("table", { name: "Tasks" });
  expect(screen.getAllByRole("button", { name: "New task" })).toHaveLength(1);
});
