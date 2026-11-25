import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useComposerChips } from "../../../product/state/composerChipStore";
import { useEditorSessions } from "../editor/editorSessions";
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

/** Answers the content route with a file record and the raw route with the bytes, and keeps every URL asked. */
function serve(file: ServedFile = {}, raw?: Uint8Array<ArrayBuffer>) {
  const asked: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    const url = String(input);
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
  return asked;
}

const query = (url: string) => new URL(url, "http://127.0.0.1").searchParams;
const contentAsk = (asked: string[]) => query(asked.find((url) => url.includes("/files/content"))!);
const rawAsks = (asked: string[]) => asked.filter((url) => url.includes("/files/raw")).map(query);

beforeEach(() => {
  useEditorSessions.setState({ sessions: {}, locked: {} });
  useComposerChips.setState({ chips: {} });
});
afterEach(() => {
  vi.restoreAllMocks();
});

it("draws another repository's image, its bytes and its read both named with the repo_id", async () => {
  const asked = serve({ binary: true, size: 2048 }, new Uint8Array([0x89, 0x50]));
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

it("still draws a text file of another repository as its own text, read with its repo_id", async () => {
  const asked = serve({ content: "# planner\n" });
  render(<Viewer threadId="t" tabId="tab" path="README.md" repoId={REPO} line={null} />);

  expect(await screen.findByRole("textbox", { name: "Edit README.md" })).toHaveValue("# planner\n");
  expect(contentAsk(asked).get("repo_id")).toBe(REPO);
  // The text tab is the editor only: it asks the raw route for nothing.
  expect(rawAsks(asked)).toEqual([]);
});

it("reads another repository's archive from the raw route with its repo_id", async () => {
  const archive = buildZipArchive([{ name: "notes.txt", data: Buffer.from("hello\n") }]);
  const asked = serve({ binary: true, size: archive.byteLength }, archive);
  render(<Viewer threadId="t" tabId="tab" path="dist/bundle.zip" repoId={REPO} line={null} />);

  expect(await screen.findByRole("button", { name: /notes\.txt/ })).toBeInTheDocument();
  const raw = rawAsks(asked);
  expect(raw.some((one) => one.get("repo_id") === REPO && one.get("path") === "dist/bundle.zip")).toBe(true);
});

it("opens another repository's Word document in the document viewer, read with its repo_id", async () => {
  stubWorker();
  const document = buildDocx(docxParagraph("Plan from the planner repository"));
  const asked = serve({ binary: true, size: document.byteLength }, document);
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
