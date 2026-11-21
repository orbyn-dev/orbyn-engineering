import { afterEach, describe, expect, it } from "vitest";
import {
  MAX_TRANSCRIPT_BYTES, looksLikeTranscript, setTranscriptsApi, splitDrop, transcriptsApi, willBeTranscript,
} from "./transcriptDrop";

const file = (name: string, size = 10) => {
  const made = new File(["x"], name);
  Object.defineProperty(made, "size", { value: size });
  return made;
};

afterEach(() => setTranscriptsApi(null));

describe("transcript drop", () => {
  it("is absent until a client is installed", () => {
    expect(transcriptsApi()).toBeNull();
  });

  it("knows a transcript by its placeholder ending, in any case", () => {
    expect(looksLikeTranscript(file("a.transcript"))).toBe(true);
    expect(looksLikeTranscript(file("A.TRANSCRIPT"))).toBe(true);
    expect(looksLikeTranscript(file("a.transcript.txt"))).toBe(false);
    expect(looksLikeTranscript(file("a.log"))).toBe(false);
  });

  it("splits a drop in the order it was made, leaving every plain file for the attachment strip", () => {
    const [one, log, two] = [file("one.transcript"), file("x.log"), file("two.transcript")];
    expect(splitDrop([one, log, two])).toEqual({ transcripts: [one, two], files: [log] });
  });

  it("keeps a 50 MiB cap", () => {
    expect(MAX_TRANSCRIPT_BYTES).toBe(50 * 1024 * 1024);
  });

  it("tells what is held from the types a browser shows during a drag, never from a name", () => {
    const item = (type: string) => ({ kind: "file", type }) as DataTransferItem;
    expect(willBeTranscript([item("application/x-orbyn-transcript")])).toBe(true);
    expect(willBeTranscript([item("")])).toBe(false);
    expect(willBeTranscript([item("application/x-orbyn-transcript"), item("text/plain")])).toBe(false);
    expect(willBeTranscript([])).toBe(false);
  });
});
