import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { Viewer } from "../files/FileViewer";
import { EditorTitleExtras } from "./EditorTitleExtras";
import { SaveChangesDialog } from "./SaveChangesDialog";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { fileKey, saveSession, useEditorSessions } from "./editorSessions";
import { useFileTabMenu } from "./useFileTabMenu";

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
      <EditorTitleExtras threadId="t" tabId="tab" onSave={() => void saveSession(KEY)} />
      <Viewer threadId="t" tabId="tab" path="auth/redirect.ts" line={null} />
    </>,
  );
}
const KEY = fileKey("t", "/repo", "auth/redirect.ts");
const editor = () => screen.findByRole("textbox", { name: "Edit auth/redirect.ts" });

beforeEach(() => {
  useEditorSessions.setState({ sessions: {}, locked: {} });
  useComposerChips.setState({ chips: {} });
  useWorkspace.setState({ threads: { t: { id: "t", cwd: "/repo" } as never } });
});
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
  expect(useEditorSessions.getState().sessions[KEY]?.text).toBe(`${TEXT}!`);
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
  expect(useEditorSessions.getState().sessions[KEY]?.text).toBe(`${TEXT}kept`);
  const second = open();
  expect(await editor()).toHaveValue(`${TEXT}kept`);
  second.unmount();

  useEditorSessions.setState({ sessions: {} });
  const third = open();
  await editor();
  third.unmount();
  expect(useEditorSessions.getState().sessions[KEY]).toBeUndefined();
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

/** Moves the pointer over the gutter at a line (19 px rows under 4 px of padding). */
const pointAt = (container: HTMLElement, line: number) => fireEvent.mouseMove(container.querySelector(".ed-gutw")!, { clientY: 4 + (line - 1) * 19 + 5 });

it("comments on a line from the gutter's +, as a chip for the next message with the file and line (editor-comment, D19)", async () => {
  serve();
  const view = open();
  await editor();
  expect(screen.queryByRole("button", { name: /^Comment on line/ })).toBeNull();
  pointAt(view.container, 3);
  await userEvent.click(screen.getByRole("button", { name: "Comment on line 3" }));
  await userEvent.type(screen.getByRole("textbox", { name: "File comment" }), "Why 25?");
  expect(screen.getByText((_, element) => element?.tagName === "LABEL" && element.textContent?.startsWith("Comment on auth/redirect.ts:3") === true)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "Add comment" }));
  expect(useComposerChips.getState().chips["t"]).toEqual([
    expect.objectContaining({ kind: "review", path: "auth/redirect.ts", line: 3, comment: "Why 25?", excerpt: "export const LIMIT = 25", sourceRevision: "r1", sourceCwd: "/repo" }),
  ]);
  expect(useUi.getState().toasts.at(-1)?.text).toBe("Review comment added to your message");
  // The line wears a dot, and the comment is listed and can be removed before sending.
  expect(view.container.querySelectorAll(".ed-dot")).toHaveLength(1);
  expect(screen.getByRole("list", { name: "File comments" })).toHaveTextContent("Why 25?");
  await userEvent.click(screen.getByRole("button", { name: "Remove comment" }));
  expect(useComposerChips.getState().chips["t"]).toEqual([]);
});

it("takes a range with Shift and refuses an empty comment", async () => {
  serve();
  const view = open();
  await editor();
  pointAt(view.container, 4);
  await userEvent.click(screen.getByRole("button", { name: "Comment on line 4" }));
  pointAt(view.container, 2);
  fireEvent.click(screen.getByRole("button", { name: "Comment on line 2" }), { shiftKey: true });
  expect(screen.getByText((_, element) => element?.tagName === "LABEL" && element.textContent?.startsWith("Comment on auth/redirect.ts:2–4") === true)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Add comment" })).toBeDisabled();
  await userEvent.type(screen.getByRole("textbox", { name: "File comment" }), "range");
  await userEvent.click(screen.getByRole("button", { name: "Add comment" }));
  expect(useComposerChips.getState().chips["t"]?.[0]).toMatchObject({ line: 2, endLine: 4, comment: "range" });
  await userEvent.click(screen.getByRole("button", { name: "Remove comment" }));
});

it("offers the cursor's line for a comment while the code has focus, so it works from the keyboard", async () => {
  serve();
  open();
  const box = await editor();
  act(() => box.focus());
  expect(screen.getByRole("button", { name: "Comment on line 1" })).toBeInTheDocument();
});

const TAB = fileTabId({ kind: "thread", path: "auth/redirect.ts" });
function Menu() {
  const items = useFileTabMenu("t", TAB, () => undefined);
  return <>{items?.map((item) => <button key={item.label} onClick={item.onSelect}>{item.label}</button>)}</>;
}

it("keeps Copy contents and Add file to message in the tab menu for a file open in the editor", async () => {
  serve();
  const writeText = vi.fn(async (_text: string) => undefined);
  Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  render(<><Viewer threadId="t" tabId={TAB} path="auth/redirect.ts" line={null} /><Menu /></>);
  const box = await editor();
  await userEvent.type(box, "!");
  await userEvent.click(screen.getByRole("button", { name: "Copy contents" }));
  // What is on disk, not the edit in progress.
  expect(writeText).toHaveBeenCalledWith(TEXT);
  await waitFor(() => expect(useUi.getState().toasts.at(-1)?.text).toBe("Contents copied"));
  await userEvent.click(screen.getByRole("button", { name: "Add file to message" }));
  expect(useComposerChips.getState().chips["t"]).toEqual([expect.objectContaining({ kind: "file", path: "auth/redirect.ts", line: null, language: "typescript" })]);
  Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
});

it("never shows or saves one thread's unsaved text in another thread's file", async () => {
  const puts: string[] = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (input: string, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("/api/editors")) return new Response("[]");
      if (init?.method === "PUT") {
        puts.push(url);
        return new Response(JSON.stringify({ path: "auth/redirect.ts", revision: "w", content: "", size: 0, binary: false, truncated: false }));
      }
      const thread = /threads\/([^/]+)\//.exec(url)?.[1] ?? "?";
      return new Response(JSON.stringify({ path: "auth/redirect.ts", revision: `r-${thread}`, content: `file of ${thread}\n`, size: 10, binary: false, truncated: false }));
    }),
  );
  useWorkspace.setState({ threads: { A: { id: "A", cwd: "/a" } as never, B: { id: "B", cwd: "/b" } as never } });
  // The same tab id names the same path in both threads.
  const first = render(<Viewer threadId="A" tabId="tab" path="auth/redirect.ts" line={null} />);
  await userEvent.type(await editor(), "x");
  first.unmount();
  const keyA = fileKey("A", "/a", "auth/redirect.ts");
  expect(useEditorSessions.getState().sessions[keyA]?.text).toBe("file of A\nx");

  render(
    <>
      <EditorTitleExtras threadId="B" tabId="tab" onSave={() => undefined} />
      <Viewer threadId="B" tabId="tab" path="auth/redirect.ts" line={null} />
    </>,
  );
  expect(await editor()).toHaveValue("file of B\n");
  // B's header sees no unsaved text, and A's session is untouched and still A's.
  expect(screen.getByRole("button", { name: "Save" })).toBeDisabled();
  expect(screen.queryByText("Unsaved", { selector: ".ed-unsaved" })).toBeNull();
  expect(useEditorSessions.getState().sessions[keyA]?.text).toBe("file of A\nx");
  // Saving A's session writes into A, whatever thread is showing.
  await saveSession(keyA);
  expect(puts).toEqual([expect.stringContaining("/threads/A/")]);
});

it("keeps a CRLF file's line ending: no false unsaved mark, and the save writes CRLF (editor-default status)", async () => {
  const served = serve({ content: "one\r\ntwo\r\n" });
  open();
  const box = await editor();
  expect(screen.getByText("TypeScript · UTF-8 · CRLF · 2 spaces")).toBeInTheDocument();
  expect(screen.getByText("Saved")).toBeInTheDocument();
  await userEvent.type(box, "x");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(served.puts).toHaveLength(1));
  expect(served.puts[0]?.content).toBe("one\r\ntwo\r\nx");
});

it("writes nothing for a locked worktree: the file reads, says Read-only and has no Save", async () => {
  serve();
  render(
    <>
      <EditorTitleExtras threadId="t" tabId="tab" onSave={() => undefined} />
      <Viewer threadId="t" tabId="tab" path="auth/redirect.ts" line={null} locked />
    </>,
  );
  const box = await screen.findByRole("textbox", { name: "Read auth/redirect.ts" });
  expect(box).toHaveAttribute("readonly");
  expect(screen.getByText("This worktree is locked. Files open read-only.")).toBeInTheDocument();
  expect(screen.getByText("Read-only", { selector: ".ed-pill" })).toBeInTheDocument();
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
  expect(useEditorSessions.getState().sessions).toEqual({});
});

it("has no Save with no thread open", () => {
  render(<EditorTitleExtras threadId={null} tabId="tab" onSave={() => undefined} />);
  expect(screen.queryByRole("button", { name: "Save" })).toBeNull();
});

it("asks the browser before the page is left with unsaved text, and not otherwise", async () => {
  serve();
  open();
  const box = await editor();
  const leave = () => {
    const event = new Event("beforeunload", { cancelable: true });
    window.dispatchEvent(event);
    return event.defaultPrevented;
  };
  expect(leave()).toBe(false);
  await userEvent.type(box, "x");
  expect(leave()).toBe(true);
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(leave()).toBe(false));
});
