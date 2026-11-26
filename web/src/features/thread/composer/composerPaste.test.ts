import type { ClipboardEvent } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { CHIP_MIME } from "../../../product/composer/composerContext";
import { handleComposerPaste } from "./composerPaste";

// A pasted chip can name another repository of the thread: its text is that repository's file, never the thread's own copy.

const carried = (chips: unknown[]) => JSON.stringify({ version: 1, chips });

function paste(payload: string) {
  const addChips = vi.fn();
  const prevented = vi.fn();
  handleComposerPaste(
    { clipboardData: { getData: (type: string) => (type === CHIP_MIME ? payload : "") }, preventDefault: prevented } as unknown as ClipboardEvent<HTMLTextAreaElement>,
    { thread: { id: "t" }, shiftPaste: { current: false }, addChips, upload: vi.fn() },
  );
  return { addChips, prevented };
}

/** The content route: every URL asked, and the file it answers. */
function serve(content: string) {
  const asked: string[] = [];
  vi.stubGlobal("fetch", vi.fn(async (input: string) => {
    asked.push(String(input));
    return new Response(JSON.stringify({ path: "notes.txt", revision: "r1", content, size: content.length, binary: false, truncated: false }));
  }));
  return asked;
}

afterEach(() => {
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

it("reads a pasted foreign file from its own repository", async () => {
  const asked = serve("planner notes\n");
  const { addChips, prevented } = paste(carried([
    { id: "f", kind: "file", path: "notes.txt", line: null, excerpt: null, language: null, repoId: "planner", repoName: "orbyn-planner" },
  ]));
  expect(prevented).toHaveBeenCalled();
  await vi.waitFor(() => expect(addChips).toHaveBeenCalled());
  expect(asked).toHaveLength(1);
  const query = new URL(asked[0]!, "http://x").searchParams;
  expect(query.get("path")).toBe("notes.txt");
  expect(query.get("repo_id")).toBe("planner");
  expect(addChips.mock.calls[0]![0]).toBe("t");
  expect(addChips.mock.calls[0]![1][0]).toMatchObject({ path: "notes.txt", excerpt: "planner notes\n", repoId: "planner", repoName: "orbyn-planner" });
});

it("reads a pasted file of the thread's own checkout with no repo_id, as before", async () => {
  const asked = serve("mine\n");
  const { addChips } = paste(carried([{ id: "f", kind: "file", path: "notes.txt", line: null, excerpt: null, language: null }]));
  await vi.waitFor(() => expect(addChips).toHaveBeenCalled());
  expect(new URL(asked[0]!, "http://x").searchParams.has("repo_id")).toBe(false);
  expect(addChips.mock.calls[0]![1][0]).toMatchObject({ path: "notes.txt", excerpt: "mine\n" });
});
