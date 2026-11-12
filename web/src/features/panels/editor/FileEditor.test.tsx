import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Viewer } from "../files/FileViewer";
import { EditorTitleExtras } from "./EditorTitleExtras";
import { SaveChangesDialog } from "./SaveChangesDialog";
import { saveSession, useEditorSessions } from "./editorSessions";

const TEXT = "import { a } from \"./a\"\n\nexport const LIMIT = 25\nexport const next = 1\nnext()\n";

interface Served {
  content?: string;
  /** What the write route answers: a status and a body, or a saved file. */
  put?: { status: number; body: unknown };
  puts: { path: string; content: string; base_revision: string | null }[];
}

/** The content route, the write route and an empty editor list. */
function serve(options: Partial<Served> = {}) {
  const state: Served = { content: TEXT, puts: [], ...options };
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("/api/editors")) return new Response("[]");
      if (init?.method === "PUT") {
        const body = JSON.parse(String(init.body));
        state.puts.push(body);
        if (state.put && state.put.status !== 200) return new Response(JSON.stringify(state.put.body), { status: state.put.status });
        return new Response(JSON.stringify({ path: body.path, revision: "r2", content: body.content, size: body.content.length, binary: false, truncated: false }));
      }
      return new Response(JSON.stringify({ path: "auth/redirect.ts", revision: "r1", content: state.content, size: 10, binary: false, truncated: false }));
    }),
  );
  return state;
}

function open() {
  return render(
    <>
      <EditorTitleExtras tabId="tab" onSave={() => void saveSession("t", "tab")} />
      <Viewer threadId="t" tabId="tab" path="auth/redirect.ts" line={null} />
    </>,
  );
}
const editor = () => screen.findByRole("textbox", { name: "Edit auth/redirect.ts" });

beforeEach(() => useEditorSessions.setState({ sessions: {}, locked: {} }));
afterEach(() => vi.restoreAllMocks());

it("opens a text file as an editor: crumb, text, cursor, language, and Saved (editor-default)", async () => {
  serve();
  open();
  expect(await editor()).toHaveValue(TEXT);
  expect(screen.getByText("redirect.ts")).toBeInTheDocument();
  expect(screen.getByText("auth")).toBeInTheDocument();
  expect(screen.getByText("Ln 1, Col 1")).toBeInTheDocument();
  expect(screen.getByText("TypeScript · UTF-8 · LF · 2 spaces")).toBeInTheDocument();
  expect(screen.getByText("Saved")).toBeInTheDocument();
  // Save is there and off until something changes.
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  // No bar of the older viewer.
  expect(screen.queryByRole("button", { name: "Copy contents" })).toBeNull();
});

it("marks unsaved changes in the header and the status bar, and saves with the revision it started from (editor-unsaved)", async () => {
  const served = serve();
  open();
  const box = await editor();
  await userEvent.type(box, "!");
  expect(screen.getByText("Unsaved", { selector: ".ed-unsaved" })).toBeInTheDocument();
  expect(screen.getAllByText("Unsaved")).toHaveLength(2);
  expect(useEditorSessions.getState().sessions["tab"]?.text).toBe(`${TEXT}!`);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(screen.getByText("Saved")).toBeInTheDocument());
  expect(served.puts).toEqual([{ path: "auth/redirect.ts", content: `${TEXT}!`, base_revision: "r1" }]);
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  // The next save starts from the revision the first one wrote.
  await userEvent.type(box, "?");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(served.puts).toHaveLength(2));
  expect(served.puts[1]?.base_revision).toBe("r2");
});

it("saves with Ctrl+S in this test window (⌘S on a Mac), and only when there is something to save", async () => {
  const served = serve();
  open();
  const box = await editor();
  fireEvent.keyDown(box, { key: "s", ctrlKey: true });
  expect(served.puts).toHaveLength(0);
  await userEvent.type(box, "x");
  fireEvent.keyDown(box, { key: "s", ctrlKey: true });
  await waitFor(() => expect(served.puts).toHaveLength(1));
});

it("keeps the text and says why when a save fails, and Try again repeats it (editor-save-error)", async () => {
  const served = serve({ put: { status: 500, body: { code: "io", message: "Permission denied (os error 13)" } } });
  open();
  await userEvent.type(await editor(), "x");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  const alert = await screen.findByRole("alert");
  expect(alert).toHaveTextContent("Couldn’t save auth/redirect.ts. The file is read-only. Your changes are still here.");
  expect(screen.getAllByText("Unsaved")).toHaveLength(2);
  served.put = undefined;
  await userEvent.click(screen.getByRole("button", { name: "Try again" }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(served.puts).toHaveLength(2);
  expect(screen.getByText("Saved")).toBeInTheDocument();
});

it("says a deleted file was deleted when the service finds nothing to write over (editor-save-error)", async () => {
  serve({ put: { status: 409, body: { code: "file_conflict", message: "changed", detail: { revision: null } } } });
  open();
  await userEvent.type(await editor(), "x");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("The file was deleted.");
});

it("asks when the file changed on disk: Reload drops mine, Keep mine saves over it (editor-conflict)", async () => {
  const served = serve({ put: { status: 409, body: { code: "file_conflict", message: "changed", detail: { revision: "r9" } } } });
  open();
  await userEvent.type(await editor(), "x");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  const banner = await screen.findByRole("alert");
  expect(banner).toHaveTextContent("Changed on disk since you opened it.");
  // Nothing is chosen for the person: the text is still mine.
  expect(await editor()).toHaveValue(`${TEXT}x`);

  served.put = undefined;
  await userEvent.click(screen.getByRole("button", { name: "Keep mine" }));
  await waitFor(() => expect(served.puts).toHaveLength(2));
  expect(served.puts[1]).toEqual({ path: "auth/redirect.ts", content: `${TEXT}x`, base_revision: "r9" });
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
});

it("Reload brings back what is on disk and clears the unsaved mark", async () => {
  const served = serve({ put: { status: 409, body: { code: "file_conflict", message: "changed", detail: { revision: "r9" } } } });
  open();
  await userEvent.type(await editor(), "x");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await screen.findByRole("alert");
  served.content = "on disk\n";
  await userEvent.click(screen.getByRole("button", { name: "Reload" }));
  await waitFor(() => expect(screen.queryByRole("alert")).toBeNull());
  expect(await editor()).toHaveValue("on disk\n");
  expect(screen.getByText("Saved")).toBeInTheDocument();
});

it("finds, counts and steps through matches, and says No matches (editor-find)", async () => {
  serve();
  open();
  await editor();
  const find = screen.getByRole("button", { name: "Find ⌘F" });
  expect(find).toHaveAttribute("aria-pressed", "false");
  await userEvent.click(find);
  expect(find).toHaveAttribute("aria-pressed", "true");
  const field = screen.getByRole("textbox", { name: "Find in file" });
  await userEvent.type(field, "next");
  expect(screen.getByText("1 of 2")).toBeInTheDocument();
  await userEvent.keyboard("{Enter}");
  expect(screen.getByText("2 of 2")).toBeInTheDocument();
  await userEvent.keyboard("{Shift>}{Enter}{/Shift}");
  expect(screen.getByText("1 of 2")).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Next match" }));
  expect(screen.getByText("2 of 2")).toBeInTheDocument();
  await userEvent.clear(field);
  await userEvent.type(field, "zzz");
  expect(screen.getByText("No matches")).toBeInTheDocument();
  expect(field).toHaveValue("zzz");
  await userEvent.keyboard("{Escape}");
  expect(screen.queryByRole("search")).toBeNull();
  expect(find).toHaveAttribute("aria-pressed", "false");
});

it("opens Find with Ctrl+F in this test window (⌘F on a Mac) from the code", async () => {
  serve();
  open();
  const box = await editor();
  fireEvent.keyDown(box, { key: "f", ctrlKey: true });
  expect(await screen.findByRole("textbox", { name: "Find in file" })).toBeInTheDocument();
});

it("goes to a line, says how many there are, and clamps a number outside them (editor-goto)", async () => {
  serve();
  open();
  const box = await editor();
  const goto = screen.getByRole("button", { name: "Go to line ⌘G" });
  await userEvent.click(goto);
  expect(goto).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByText("1 to 6")).toBeInTheDocument();
  const field = screen.getByRole("textbox", { name: "Line number" });
  await userEvent.clear(field);
  await userEvent.type(field, "4{Enter}");
  expect(screen.getByText("Ln 4, Col 1")).toBeInTheDocument();
  await userEvent.click(goto);
  await userEvent.click(goto);
  await userEvent.clear(screen.getByRole("textbox", { name: "Line number" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Line number" }), "999{Enter}");
  expect(screen.getByRole("textbox", { name: "Line number" })).toHaveValue("6");
  expect(screen.getByText("Ln 6, Col 1")).toBeInTheDocument();
  expect(box).toBeInTheDocument();
});

it("shows the wait as grey lines only once it passes 400 ms (editor-loading)", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("fetch", vi.fn(() => new Promise(() => undefined)));
  const view = open();
  expect(screen.getByText("Loading file")).toBeInTheDocument();
  expect(view.container.querySelectorAll(".ed-skel i")).toHaveLength(0);
  expect(screen.getByText("Opening…")).toBeInTheDocument();
  await act(async () => vi.advanceTimersByTime(450));
  expect(view.container.querySelectorAll(".ed-skel i")).toHaveLength(10);
  vi.useRealTimers();
});

it("keeps unsaved text when the tab is left and shown again, and drops a clean one", async () => {
  serve();
  const first = open();
  await userEvent.type(await editor(), "kept");
  first.unmount();
  expect(useEditorSessions.getState().sessions["tab"]?.text).toBe(`${TEXT}kept`);
  const second = open();
  expect(await editor()).toHaveValue(`${TEXT}kept`);
  second.unmount();

  useEditorSessions.setState({ sessions: {} });
  const third = open();
  await editor();
  third.unmount();
  expect(useEditorSessions.getState().sessions["tab"]).toBeUndefined();
});

it("asks Save, Don’t save or Cancel, with Save as the default (editor-close-confirm)", async () => {
  const onSave = vi.fn();
  const onDiscard = vi.fn();
  const onCancel = vi.fn();
  render(<SaveChangesDialog path="auth/redirect.ts" onSave={onSave} onDiscard={onDiscard} onCancel={onCancel} />);
  expect(screen.getByRole("dialog", { name: "Save changes to redirect.ts?" })).toBeInTheDocument();
  expect(screen.getByText("Your changes to auth/redirect.ts will be lost if you don’t save them.")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Save" })).toHaveFocus();
  fireEvent.keyDown(document, { key: "Enter", metaKey: true });
  expect(onSave).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole("button", { name: "Don’t save" }));
  expect(onDiscard).toHaveBeenCalledTimes(1);
  await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(onCancel).toHaveBeenCalledTimes(1);
});
