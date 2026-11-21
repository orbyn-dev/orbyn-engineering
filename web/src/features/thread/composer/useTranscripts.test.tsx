import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { ApiFailure } from "../../../api/client";
import { MAX_TRANSCRIPT_BYTES, setTranscriptsApi } from "./transcriptDrop";
import { useTranscripts } from "./useTranscripts";

const thread = { id: "t1" };
const file = (name: string, size = 10) => {
  const made = new File(["x"], name);
  Object.defineProperty(made, "size", { value: size });
  return made;
};
const ready = (title: string) => ({ id: `id:${title}`, title, message_count: 3 });

afterEach(() => setTranscriptsApi(null));

it("does nothing without a client: every file is left for the attachment strip", () => {
  const { result } = renderHook(() => useTranscripts(thread));
  let left: File[] = [];
  act(() => { left = result.current.take([file("a.transcript"), file("b.log")]); });
  expect(left.map((f) => f.name)).toEqual(["a.transcript", "b.log"]);
  expect(result.current.entries).toEqual([]);
});

it("takes the transcripts of a drop in order, hands back the rest, and holds send while one is read", async () => {
  let finish!: (value: unknown) => void;
  const read = vi.fn().mockReturnValueOnce(new Promise((done) => { finish = done; })).mockResolvedValueOnce(ready("Two"));
  setTranscriptsApi({ read });
  const { result } = renderHook(() => useTranscripts(thread));
  let left: File[] = [];
  act(() => { left = result.current.take([file("one.transcript"), file("x.log"), file("two.transcript")]); });
  expect(left.map((f) => f.name)).toEqual(["x.log"]);
  expect(result.current.reading).toBe(true);
  await act(async () => finish(ready("One")));
  await waitFor(() => expect(result.current.reading).toBe(false));
  expect(result.current.entries.map((e) => e.state === "ready" && e.title)).toEqual(["One", "Two"]);
  expect(result.current.ids).toEqual(["id:One", "id:Two"]);
});

it("refuses a file over 50 MiB without sending it", () => {
  const read = vi.fn();
  setTranscriptsApi({ read });
  const { result } = renderHook(() => useTranscripts(thread));
  act(() => { result.current.take([file("all-hands.transcript", MAX_TRANSCRIPT_BYTES + 1)]); });
  expect(read).not.toHaveBeenCalled();
  expect(result.current.entries[0]).toMatchObject({ state: "refused", reason: "Files can be up to 50 MiB.", canAddAsFile: false });
  expect(result.current.reading).toBe(false);
});

it.each([
  ["transcript_too_new", "Made by a newer Orbyn. Update Orbyn to read it.", false],
  ["transcript_unreadable", "This isn’t a transcript Orbyn can read.", true],
  ["file_too_large", "Files can be up to 50 MiB.", false],
])("says why %s in the chip, and offers a plain file only for a damaged one", async (code, reason, plain) => {
  setTranscriptsApi({ read: vi.fn().mockRejectedValue(new ApiFailure(code, "words from the server", 422)) });
  const { result } = renderHook(() => useTranscripts(thread));
  act(() => { result.current.take([file("a.transcript")]); });
  await waitFor(() => expect(result.current.entries[0]?.state).toBe("refused"));
  expect(result.current.entries[0]).toMatchObject({ reason, canAddAsFile: plain });
  expect(result.current.ids).toEqual([]);
});

it("removes a chip on its own, and drops the ones that were sent", async () => {
  setTranscriptsApi({ read: vi.fn().mockImplementation(async (_t: string, f: File) => ready(f.name)) });
  const { result } = renderHook(() => useTranscripts(thread));
  act(() => { result.current.take([file("a.transcript"), file("b.transcript")]); });
  await waitFor(() => expect(result.current.ids).toHaveLength(2));
  act(() => result.current.remove(result.current.entries[0]!.key));
  expect(result.current.ids).toEqual(["id:b.transcript"]);
  act(() => result.current.consume());
  expect(result.current.entries).toEqual([]);
});
