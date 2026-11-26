import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { chipLabel, chipPrompt } from "../../../product/composer/composerContext";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { fileTabId, useUi } from "../../../product/state/ui";
import { useWorkspace } from "../../../product/state/workspace";
import { useEditorSessions } from "../editor/editorSessions";
import { FileTab } from "./FileTab";
import { Viewer } from "./FileViewer";
import { buildZipArchive } from "./zipArchive.test-support";
import { buildDocx, docxParagraph, stubWorker } from "./office.test-support";

// F70 (#1060): another repository's file tab previews its media, archive and document from the raw route, which
// carries the tab's repo_id; the thread's own checkout keeps sending none.
const REPO = "planner";

interface ServedFile {
  content?: string;
  binary?: boolean;
  truncated?: boolean;
  size?: number;
}

/** Answers the content route with a file record and the raw route with the bytes, and keeps every URL asked and body written. */
function serve(file: ServedFile = {}, raw?: Uint8Array<ArrayBuffer>) {
  const asked: string[] = [];
  const puts: Record<string, unknown>[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string, init?: RequestInit) => {
    const url = String(input);
    if (init?.method === "PUT") {
      const body = JSON.parse(String(init.body));
      puts.push(body);
      return new Response(JSON.stringify({ path: body.path, revision: "r2", content: body.content, size: 1, binary: false, truncated: false }));
    }
    asked.push(url);
    if (url.includes("/files/raw")) {
      return raw
        ? new Response(raw.slice().buffer)
        : new Response(JSON.stringify({ code: "not_found", message: "not found" }), { status: 404 });
    }
    if (url.includes("/api/editors")) return new Response("[]");
    return new Response(JSON.stringify({
      path: "file",
      revision: "r1",
      content: file.content ?? "",
      size: file.size ?? 1024,
      binary: file.binary ?? false,
      truncated: file.truncated ?? false,
    }));
  }));
  return { asked, puts };
}

const query = (url: string) => new URL(url, "http://127.0.0.1").searchParams;
const contentAsk = (asked: string[]) => query(asked.find((url) => url.includes("/files/content"))!);
const rawAsks = (asked: string[]) => asked.filter((url) => url.includes("/files/raw")).map(query);

beforeEach(() => {
  useEditorSessions.setState({ sessions: {}, locked: {} });
  useComposerChips.setState({ chips: {} });
  useWorkspace.setState({ threads: { t: { id: "t", project_id: "p", cwd: "/repo" } as never } });
});
afterEach(() => {
  vi.restoreAllMocks();
});

it("draws another repository's image, its bytes and its read both named with the repo_id", async () => {
  const { asked } = serve({ binary: true, size: 2048 }, new Uint8Array([0x89, 0x50]));
  render(<Viewer threadId="t" tabId="tab" path="art/logo.png" repoId={REPO} line={null} />);

  const img = await screen.findByRole("img");
  const src = query(img.getAttribute("src")!);
  expect(src.get("path")).toBe("art/logo.png");
  expect(src.get("repo_id")).toBe(REPO);
  // The read names it too, so an untrusted or missing repository is said in words instead of a broken image.
  expect(contentAsk(asked).get("repo_id")).toBe(REPO);
});

it("draws the thread's own image from the raw route with no repo_id, as before", async () => {
  serve({ binary: true, size: 12 }, new Uint8Array([0x89]));
  render(<Viewer threadId="t" tabId="tab" path="art/logo.png" line={null} />);

  const img = await screen.findByRole("img");
  expect(query(img.getAttribute("src")!).has("repo_id")).toBe(false);
});

it("puts the repo_id on the download link of another repository's file", async () => {
  serve({ binary: true, size: 64 }, new Uint8Array([1, 2]));
  render(<Viewer threadId="t" tabId="tab" path="art/logo.png" repoId={REPO} line={null} />);

  const save = await screen.findByRole("link", { name: "Save file" });
  const href = query(save.getAttribute("href")!);
  expect(href.get("path")).toBe("art/logo.png");
  expect(href.get("repo_id")).toBe(REPO);
  expect(href.get("download")).toBe("true");
});

it("puts the repo_id on a binary file's own Save link, where there is no media bar", async () => {
  serve({ binary: true, size: 64 }, new Uint8Array([0x00, 0x01]));
  render(<Viewer threadId="t" tabId="tab" path="bin/data.bin" repoId={REPO} line={null} />);

  expect(await screen.findByText(/data\.bin is a binary file/)).toBeInTheDocument();
  const save = screen.getByRole("link", { name: "Save file" });
  const href = query(save.getAttribute("href")!);
  expect(href.get("path")).toBe("bin/data.bin");
  expect(href.get("repo_id")).toBe(REPO);
  expect(href.get("download")).toBe("true");
  // The editor-only binary frame carries the save; no media bar is drawn above it.
  expect(screen.queryByRole("button", { name: "Copy contents" })).toBeNull();
});

it("renders another repository's Markdown from the same read, named with its repo_id", async () => {
  const { asked } = serve({ content: "# planner\n\nThe plan.\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" repoId={REPO} line={null} />);

  expect(await screen.findByRole("heading", { name: "planner" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "Preview" })).toBeChecked();
  expect(screen.getByRole("radio", { name: "Source" })).toBeInTheDocument();
  expect(screen.getByRole("radio", { name: "Edit" })).toBeInTheDocument();
  expect(contentAsk(asked).get("repo_id")).toBe(REPO);
  // Rendered from the content read, so the raw route is asked for nothing.
  expect(rawAsks(asked)).toEqual([]);
});

it.each(["orbyn-planner", `repo-${"x".repeat(59)}`])("adds another repository's file to the message under its full repository name %s", async (repoName) => {
  serve({ content: "# planner\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" repoId={REPO} repoName={repoName} line={null} />);

  await screen.findByRole("heading", { name: "planner" });
  await userEvent.click(screen.getByRole("button", { name: "Add file to message" }));

  const chip = useComposerChips.getState().chips.t![0]!;
  // The chip names the tab's repository, client-only, so the next message points at the right README.md.
  expect(chip).toMatchObject({ kind: "file", path: "README.md", line: null, repoId: REPO, repoName });
  expect(chipLabel(chip)).toBe(`${repoName}/README.md`);
  expect(chipPrompt(chip)).toContain(`File \`${repoName}/README.md\``);
});

it("keeps Add file to message off on another repository's tab until the panel names the repository", async () => {
  serve({ content: "# planner\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" repoId={REPO} line={null} />);

  await screen.findByRole("heading", { name: "planner" });
  // A path alone would read as the thread's own file of the same name, so the chip waits for the repository's name.
  expect(screen.getByRole("button", { name: "Add file to message" })).toBeDisabled();
});

it("adds the thread's own file to the message with no repository, as before", async () => {
  serve({ content: "# mine\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" line={null} />);

  await screen.findByRole("heading", { name: "mine" });
  await userEvent.click(screen.getByRole("button", { name: "Add file to message" }));

  const chip = useComposerChips.getState().chips.t![0]!;
  expect(chip).toMatchObject({ kind: "file", path: "README.md", line: null });
  expect(chip).not.toHaveProperty("repoId");
  expect(chip).not.toHaveProperty("repoName");
  expect(chipLabel(chip)).toBe("README.md");
});

it("draws another repository's plain text file as the editor, read with its repo_id", async () => {
  const { asked } = serve({ content: "notes\n" });
  render(<Viewer threadId="t" tabId="tab" path="notes.txt" repoId={REPO} line={null} />);

  expect(await screen.findByRole("textbox", { name: "Edit notes.txt" })).toHaveValue("notes\n");
  expect(contentAsk(asked).get("repo_id")).toBe(REPO);
});

it("saves an edit of another repository's rendered file with its repo_id in the body", async () => {
  const { puts } = serve({ content: "# planner\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" repoId={REPO} line={null} />);

  await screen.findByRole("heading", { name: "planner" });
  await userEvent.click(screen.getByRole("radio", { name: "Edit" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Edit README.md" }), "more");
  await userEvent.click(screen.getByRole("button", { name: "Save" }));
  await waitFor(() => expect(puts).toHaveLength(1));
  expect(puts[0]).toEqual({ path: "README.md", content: "# planner\nmore", base_revision: "r1", repo_id: REPO });
});

it("makes a line comment on another repository's file name its repository", async () => {
  serve({ content: "# one\ntwo\nthree\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" repoId={REPO} repoName="orbyn-planner" line={null} />);

  await screen.findByRole("heading", { name: "one" });
  await userEvent.click(screen.getByRole("radio", { name: "Source" }));
  await userEvent.click(screen.getByRole("button", { name: "Comment on line 2" }));
  await userEvent.type(screen.getByRole("textbox", { name: "File comment" }), "Why here?");
  await userEvent.click(screen.getByRole("button", { name: "Add comment" }));

  const chip = useComposerChips.getState().chips.t![0]!;
  // The comment names the tab's repository, so the next message points at the right file of the thread.
  expect(chip).toMatchObject({ kind: "review", path: "README.md", line: 2, comment: "Why here?", repoId: REPO, repoName: "orbyn-planner", sourceRevision: "r1", sourceCwd: "/repo" });
  expect(chipLabel(chip)).toBe("orbyn-planner/README.md:2");
  expect(chipPrompt(chip)).toContain("Review on `orbyn-planner/README.md:2`");
  expect(screen.getByRole("list", { name: "File comments" })).toHaveTextContent("orbyn-planner/README.md:2");
});

it("keeps the thread's own Markdown preview and its line comments exactly as they were", async () => {
  const { asked } = serve({ content: "# mine\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" line={null} />);

  expect(await screen.findByRole("heading", { name: "mine" })).toBeInTheDocument();
  expect(contentAsk(asked).has("repo_id")).toBe(false);
  await userEvent.click(screen.getByRole("radio", { name: "Source" }));
  await userEvent.click(screen.getByRole("button", { name: "Comment on line 1" }));
  await userEvent.type(screen.getByRole("textbox", { name: "File comment" }), "why?");
  await userEvent.click(screen.getByRole("button", { name: "Add comment" }));

  const chip = useComposerChips.getState().chips.t![0]!;
  expect(chip).toEqual({ id: expect.any(String), kind: "review", path: "README.md", line: 1, endLine: 1, comment: "why?", excerpt: "# mine", sourceRevision: "r1", sourceCwd: "/repo" });
  expect(chip).not.toHaveProperty("repoId");
  expect(chip).not.toHaveProperty("repoName");
  expect(chipLabel(chip)).toBe("README.md:1");
});

it("opens another repository's tab for its own comment reveal, and the thread's tab takes only its own", async () => {
  serve({ content: "one\ntwo\n" });
  const consume = vi.spyOn(useUi.getState(), "consumeFileReveal");
  const id = fileTabId({ kind: "thread", path: "notes.txt", repoId: REPO });
  const view = render(<FileTab id={id} threadId="t" repoName="orbyn-planner" />);
  await screen.findByRole("textbox", { name: "Edit notes.txt" });
  act(() => useUi.getState().revealFile("notes.txt", 2, { threadId: "t", cwd: "/repo" }, REPO));
  expect(consume).toHaveBeenCalled();
  view.unmount();
  consume.mockClear();

  // The thread's own tab of the same path is a different file: a repository's reveal is not its to take.
  act(() => useUi.getState().revealFile("notes.txt", 2, { threadId: "t", cwd: "/repo" }, REPO));
  render(<FileTab id={fileTabId({ kind: "thread", path: "notes.txt" })} threadId="t" />);
  await act(async () => undefined);
  expect(consume).not.toHaveBeenCalled();
  expect(useUi.getState().fileReveal?.repoId).toBe(REPO);
});

it("reads another repository's archive from the raw route with its repo_id", async () => {
  const archive = buildZipArchive([{ name: "notes.txt", data: Buffer.from("hello\n") }]);
  const { asked } = serve({ binary: true, size: archive.byteLength }, archive);
  render(<Viewer threadId="t" tabId="tab" path="dist/bundle.zip" repoId={REPO} line={null} />);

  expect(await screen.findByRole("button", { name: /notes\.txt/ })).toBeInTheDocument();
  const raw = rawAsks(asked);
  expect(raw.some((one) => one.get("repo_id") === REPO && one.get("path") === "dist/bundle.zip")).toBe(true);
});

it("opens another repository's Word document in the document viewer, read with its repo_id", async () => {
  stubWorker();
  const document = buildDocx(docxParagraph("Plan from the planner repository"));
  const { asked } = serve({ binary: true, size: document.byteLength }, document);
  render(<Viewer threadId="t" tabId="tab" path="docs/plan.docx" repoId={REPO} line={null} />);

  expect(await screen.findByText("Plan from the planner repository")).toBeInTheDocument();
  expect(rawAsks(asked).some((one) => one.get("repo_id") === REPO)).toBe(true);
});

it("offers the outside editor for another repository as it does for the thread's own file", async () => {
  // The request body that carries the repo_id is checked in client.test.ts and FileEditor.repo.test.tsx.
  serve({ binary: true, size: 64 }, new Uint8Array([1, 2]));
  const foreign = render(<Viewer threadId="t" tabId="tab" path="art/logo.png" repoId={REPO} line={null} />);
  expect(await screen.findByRole("button", { name: "Open in editor" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: /not available for another repository/ })).toBeNull();
  foreign.unmount();

  serve({ binary: true, size: 64 }, new Uint8Array([1, 2]));
  render(<Viewer threadId="t" tabId="tab" path="art/logo.png" line={null} />);
  expect(await screen.findByRole("button", { name: "Open in editor" })).toBeEnabled();
});

it("says an untrusted repository in words rather than only failing to load the media", async () => {
  vi.stubGlobal("fetch", vi.fn(async () =>
    new Response(JSON.stringify({ code: "repo_untrusted", message: "orbyn-planner is not trusted." }), { status: 409 })));
  render(<Viewer threadId="t" tabId="tab" path="art/logo.png" repoId={REPO} line={null} />);

  expect(await screen.findByText(/orbyn-planner is not trusted\./)).toBeInTheDocument();
  expect(screen.queryByRole("img")).toBeNull();
});
