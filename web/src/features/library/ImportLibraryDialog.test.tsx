import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { createLibraryMock, type LibraryMock } from "../../../mock/library";
import { ApiFailure, NETWORK_ERROR } from "../../api/client";
import { ImportLibraryDialog } from "./ImportLibraryDialog";
import { setLibraryApi } from "./api";
import type { ImportAnswer } from "./contract";

afterEach(() => { setLibraryApi(null); });

const EXPORT = { format: "orbyn-library", version: 1, exported_at: "2026-11-22T00:00:00Z", profiles: [], workflows: [], prompts: [], env_sets: [] };
const ONE = { ...EXPORT, workflows: [{ name: "New one", description: "", command: "echo 1", params: [], env_set_names: [] }] };

const file = (body: unknown, name = "orbyn-library-2026-11-22.json") =>
  new File([typeof body === "string" ? body : JSON.stringify(body)], name, { type: "application/json" });

const answer = (over: Partial<ImportAnswer> = {}): ImportAnswer => ({ dry_run: true, problems: [], created: 0, renamed: 0, refused: 0, lines: [], ...over });

const mount = (options: { file?: File; limit?: number; mock?: LibraryMock } = {}) => {
  const mock = options.mock ?? createLibraryMock();
  setLibraryApi(mock);
  const onClose = vi.fn();
  const onApplied = vi.fn();
  const onChooseAnother = vi.fn();
  const user = userEvent.setup();
  const view = render(
    <ImportLibraryDialog
      file={options.file ?? file(EXPORT)}
      limit={options.limit ?? 2 * 1024 * 1024}
      onClose={onClose}
      onApplied={onApplied}
      onChooseAnother={onChooseAnother}
    />,
  );
  return { ...view, mock, user, onClose, onApplied, onChooseAnother };
};

const cells = (row: HTMLElement) => within(row).getAllByRole("cell").map((cell) => cell.textContent);

it("reads the export and draws what the dry run would create, per kind, writing nothing", async () => {
  const mock = createLibraryMock({ delayMs: 20 });
  const importLibrary = vi.spyOn(mock, "importLibrary");
  const document = { ...EXPORT, workflows: [{ name: "New one", description: "", command: "echo 1", params: [], env_set_names: [] }], prompts: [{ name: "Ask", description: "", body: "x", params: [] }], env_sets: [{ name: "Staging", description: "", keys: ["API_TOKEN"] }] };
  mount({ mock, file: file(document) });
  expect(await screen.findByText("Reading the file…")).toBeVisible();
  const table = await screen.findByRole("table", { name: "Import by kind" });
  expect(within(table).getByRole("columnheader", { name: "Kind" })).toBeVisible();
  expect(cells(within(table).getByRole("row", { name: /^Workflows/ }))).toEqual(["1", "0", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^Prompts/ }))).toEqual(["1", "0", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^Env sets/ }))).toEqual(["1", "0", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^All/ }))).toEqual(["3", "0", "0"]);
  expect(screen.getByText("Checked by the service. Nothing is written until you choose Import.")).toBeVisible();
  expect(importLibrary).toHaveBeenCalledWith(document, true);
  expect(mock.writes).toEqual([]);
  expect(screen.getByRole("button", { name: "Import 3 items" })).toBeEnabled();
});

it("refuses a file that is not JSON, with no request and a way to choose another", async () => {
  const mock = createLibraryMock();
  const importLibrary = vi.spyOn(mock, "importLibrary");
  const { user, onChooseAnother } = mount({ mock, file: file("this is not json") });
  expect(await screen.findByText("This file is not JSON.")).toBeVisible();
  expect(screen.getByText("Import reads an Orbyn library export file.")).toBeVisible();
  expect(importLibrary).not.toHaveBeenCalled();
  expect(screen.queryByRole("table")).toBeNull();
  await user.click(screen.getByRole("button", { name: "Choose another file…" }));
  expect(onChooseAnother).toHaveBeenCalledTimes(1);
});

it("shows the service's words for a JSON document that is not a library export", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => { throw new ApiFailure("invalid_input", "document: this is not an Orbyn library export this version reads.", 400); };
  mount({ mock, file: file({ version: 1, workflows: [] }) });
  expect(await screen.findByText("The service refused this document.")).toBeVisible();
  expect(screen.getByText("document: this is not an Orbyn library export this version reads.")).toBeVisible();
  expect(screen.queryByRole("button", { name: /^Import/ })).toBeNull();
  expect(screen.getByRole("button", { name: "Close" })).toBeVisible();
});

it("draws the counts per kind, the renamed names, the refused entries and the dropped parts", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => answer({
    created: 2,
    renamed: 1,
    refused: 2,
    lines: [
      { kind: "workflow", index: 0, outcome: "created", name: "Keep" },
      { kind: "workflow", index: 1, outcome: "renamed", name: "Tests (2)" },
      { kind: "prompt", index: 0, outcome: "created", name: "Ask" },
      { kind: "profile", index: 0, outcome: "refused", code: "invalid_input" },
      { kind: "env_set", index: 2, outcome: "refused", code: "library_limit" },
    ],
    problems: [
      { kind: "workflow", index: 2, code: "unknown_env_set" },
      { kind: "env_set", index: 0, code: "value_dropped" },
    ],
  });
  mount({ mock, file: file(ONE) });
  const table = await screen.findByRole("table", { name: "Import by kind" });
  expect(cells(within(table).getByRole("row", { name: /^Profiles/ }))).toEqual(["0", "0", "1"]);
  expect(cells(within(table).getByRole("row", { name: /^Workflows/ }))).toEqual(["1", "1", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^Prompts/ }))).toEqual(["1", "0", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^Env sets/ }))).toEqual(["0", "0", "1"]);
  expect(screen.getByText(/Saved under a free name: Tests \(2\)/)).toBeVisible();
  expect(screen.getByRole("heading", { name: "Refused · 2" })).toBeVisible();
  const refused = screen.getByRole("list", { name: "Refused entries" });
  expect(within(refused).getByText("Profiles #0")).toBeVisible();
  expect(within(refused).getByText("invalid_input")).toBeVisible();
  expect(within(refused).getByText("Env sets #2")).toBeVisible();
  expect(within(refused).getByText("library_limit")).toBeVisible();
  expect(screen.getByRole("heading", { name: "Dropped · 2" })).toBeVisible();
  const dropped = screen.getByRole("list", { name: "Dropped parts" });
  expect(within(dropped).getByText("Workflows #2")).toBeVisible();
  expect(within(dropped).getByText("unknown_env_set")).toBeVisible();
  expect(within(dropped).getByText("a set this computer does not have, so that link is left out.")).toBeVisible();
  expect(within(dropped).getByText("value_dropped")).toBeVisible();
  expect(within(dropped).getByText("an environment value; only key names are imported.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Import 3 items" })).toBeEnabled();
});

it("draws the profile, dropped-value and unknown-set rows the dev mock answers for a crafted export", async () => {
  const mock = createLibraryMock({ profiles: ["reviewer"] });
  mount({ mock, file: file({
    format: "orbyn-library",
    version: 1,
    exported_at: "2026-11-22T00:00:00Z",
    profiles: [{ name: "Reviewer", description: "", engine: null, model_tier: "balanced", permission_mode: "ask", autopilot_mode: "off" }],
    env_sets: [{ name: "Staging", description: "", keys: [{ name: "API_TOKEN", value: "s3cr3t" }] }],
    workflows: [{ name: "Deploy", description: "", command: "deploy", params: [], env_set_names: ["Missing"] }],
    prompts: [],
  }) });
  const table = await screen.findByRole("table", { name: "Import by kind" });
  expect(cells(within(table).getByRole("row", { name: /^Profiles/ }))).toEqual(["0", "1", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^Workflows/ }))).toEqual(["1", "0", "0"]);
  expect(cells(within(table).getByRole("row", { name: /^Env sets/ }))).toEqual(["1", "0", "0"]);
  expect(screen.getByText(/Saved under a free name: Reviewer \(2\)/)).toBeVisible();
  const dropped = screen.getByRole("list", { name: "Dropped parts" });
  expect(within(dropped).getByText("Env sets #0")).toBeVisible();
  expect(within(dropped).getByText("value_dropped")).toBeVisible();
  expect(within(dropped).getByText("Workflows #0")).toBeVisible();
  expect(within(dropped).getByText("unknown_env_set")).toBeVisible();
  expect(document.body.textContent).not.toContain("s3cr3t");
  expect(screen.getByRole("button", { name: "Import 3 items" })).toBeEnabled();
  expect(mock.writes).toEqual([]);
});

it("posts the dry run first and the real import only when Import is clicked, then reports it", async () => {
  const mock = createLibraryMock();
  const importLibrary = vi.spyOn(mock, "importLibrary");
  const { user, onApplied } = mount({ mock, file: file(ONE) });
  expect(screen.queryByRole("table")).toBeNull();
  expect(mock.writes).toEqual([]);
  await user.click(await screen.findByRole("button", { name: "Import 1 item" }));
  await waitFor(() => expect(mock.writes).toEqual(["import:1+0"]));
  expect(importLibrary.mock.calls.map((call) => call[1])).toEqual([true, false]);
  expect(onApplied).toHaveBeenCalledTimes(1);
  expect(onApplied.mock.calls[0]![0]).toMatchObject({ dry_run: false, created: 1 });
  expect(screen.getByText("Imported 1 item to Personal.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Close" })).toBeVisible();
  expect(screen.queryByRole("button", { name: /^Import/ })).toBeNull();
});

it("imports once for a double click", async () => {
  const mock = createLibraryMock({ delayMs: 20 });
  const importLibrary = vi.spyOn(mock, "importLibrary");
  const { user } = mount({ mock, file: file(ONE) });
  await user.dblClick(await screen.findByRole("button", { name: "Import 1 item" }));
  await waitFor(() => expect(mock.writes).toEqual(["import:1+0"]));
  expect(importLibrary.mock.calls.filter((call) => call[1] === false)).toHaveLength(1);
});

it("keeps Import off when the dry run would create nothing", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => answer({ refused: 1, lines: [{ kind: "prompt", index: 0, outcome: "refused", code: "invalid_input" }] });
  mount({ mock, file: file(EXPORT) });
  const button = await screen.findByRole("button", { name: "Nothing to import" });
  expect(button).toBeDisabled();
  expect(screen.getByText("The service refused these entries. Nothing was written for them.")).toBeVisible();
});

it("says an older service has no import, and offers no other file", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => { throw new ApiFailure("not_found", "No such route.", 404); };
  mount({ mock, file: file(EXPORT) });
  expect(await screen.findByText("This version of the Orbyn service has no library import.")).toBeVisible();
  expect(screen.getByText("Update Orbyn, then try again.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Choose another file…" })).toBeNull();
  expect(screen.queryByRole("button", { name: /^Import/ })).toBeNull();
});

it("shows a 409 refusal in the service's words", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => { throw new ApiFailure("library_limit", "The library is full for this kind.", 409); };
  mount({ mock, file: file(EXPORT) });
  expect(await screen.findByText("The service refused the import.")).toBeVisible();
  expect(screen.getByText("The library is full for this kind.")).toBeVisible();
  expect(screen.getByRole("button", { name: "Choose another file…" })).toBeVisible();
});

it("shows a 422 refusal and a network failure honestly", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => { throw new ApiFailure("invalid_input", "document: the file could not be read.", 422); };
  const view = mount({ mock, file: file(EXPORT) });
  expect(await screen.findByText("The service couldn’t read this document.")).toBeVisible();
  expect(screen.getByText("document: the file could not be read.")).toBeVisible();
  setLibraryApi(mock);
  mock.importLibrary = async () => { throw new ApiFailure(NETWORK_ERROR, "Can't reach the Orbyn service.", 0); };
  view.unmount();
  mount({ mock, file: file(EXPORT) });
  expect(await screen.findByText("Can’t reach the Orbyn service.")).toBeVisible();
  expect(screen.getByText("Check that Orbyn is running, then try again.")).toBeVisible();
});

it("keeps the preview with the service's words when the import itself fails", async () => {
  const mock = createLibraryMock();
  let calls = 0;
  mock.importLibrary = async (_document, dryRun) => {
    calls += 1;
    if (dryRun) return answer({ created: 1, lines: [{ kind: "workflow", index: 0, outcome: "created", name: "New one" }] });
    throw new ApiFailure("library_limit", "The library is full for this kind.", 409);
  };
  const { user, onApplied } = mount({ mock, file: file(ONE) });
  await user.click(await screen.findByRole("button", { name: "Import 1 item" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The service refused the import. The library is full for this kind.");
  expect(onApplied).not.toHaveBeenCalled();
  expect(screen.getByRole("table", { name: "Import by kind" })).toBeVisible();
  expect(screen.getByRole("button", { name: "Import 1 item" })).toBeEnabled();
  expect(calls).toBe(2);
});

it("refuses a list entry that is not an object, instead of crashing the report", async () => {
  const bad: Partial<ImportAnswer>[] = [
    { created: 1, lines: [null] as unknown as ImportAnswer["lines"] },
    { created: 1, lines: [{ index: 0, outcome: "refused", code: "invalid_input" }] as unknown as ImportAnswer["lines"] },
    { created: 1, lines: [], problems: [null] as unknown as ImportAnswer["problems"] },
  ];
  const mock = createLibraryMock();
  for (const report of bad) {
    mock.importLibrary = async () => answer(report);
    const view = mount({ mock, file: file(ONE) });
    expect(await screen.findByText("The service answered an import in a shape this app does not know.")).toBeVisible();
    expect(screen.queryByRole("table")).toBeNull();
    view.unmount();
  }
});

it("refuses an answer that is not the report, and a dry run that says it wrote", async () => {
  const mock = createLibraryMock();
  mock.importLibrary = async () => ({ hello: "world" }) as unknown as ImportAnswer;
  const view = mount({ mock, file: file(ONE) });
  expect(await screen.findByText("The service answered an import in a shape this app does not know.")).toBeVisible();
  expect(screen.getByText("Update Orbyn, then try again.")).toBeVisible();
  view.unmount();
  mock.importLibrary = async () => answer({ dry_run: false, created: 1, lines: [{ kind: "workflow", index: 0, outcome: "created", name: "New one" }] });
  mount({ mock, file: file(ONE) });
  expect(await screen.findByText("The service answered an import in a shape this app does not know.")).toBeVisible();
  expect(mock.writes).toEqual([]);
});

it("refuses a file over the service's cap without reading it", async () => {
  const mock = createLibraryMock();
  const importLibrary = vi.spyOn(mock, "importLibrary");
  const big = file("{}");
  Object.defineProperty(big, "size", { value: 2 * 1024 * 1024 + 1 });
  const read = vi.spyOn(big, "text");
  mount({ mock, file: big });
  expect(await screen.findByText("This file is too large to import.")).toBeVisible();
  expect(screen.getByText("An import file is at most 2 MiB.")).toBeVisible();
  expect(read).not.toHaveBeenCalled();
  expect(importLibrary).not.toHaveBeenCalled();
});

it("refuses a file whose request body would exceed the cap, though the file itself fits", async () => {
  const mock = createLibraryMock();
  const importLibrary = vi.spyOn(mock, "importLibrary");
  const document = { ...ONE, workflows: [{ ...ONE.workflows[0]!, description: "x".repeat(200) }] };
  const fileSize = new TextEncoder().encode(JSON.stringify(document)).length;
  const limit = fileSize + 4; // The file fits; the ",\"dry_run\":false" the client adds does not.
  mount({ mock, file: file(document), limit });
  expect(await screen.findByText("This file is too large to import.")).toBeVisible();
  expect(screen.getByText(`An import file is at most ${limit} B.`)).toBeVisible();
  expect(importLibrary).not.toHaveBeenCalled();
});
