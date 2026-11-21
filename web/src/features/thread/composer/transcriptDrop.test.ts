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

  it("knows a transcript by its ending, in any case", () => {
    expect(looksLikeTranscript(file("a.orbyn.json"))).toBe(true);
    expect(looksLikeTranscript(file("A.ORBYN.JSON"))).toBe(true);
    expect(looksLikeTranscript(file("a.orbyn.json.txt"))).toBe(false);
    expect(looksLikeTranscript(file("a.log"))).toBe(false);
  });

  it("splits a drop in the order it was made, leaving every plain file for the attachment strip", () => {
    const [one, log, two] = [file("one.orbyn.json"), file("x.log"), file("two.orbyn.json")];
    expect(splitDrop([one, log, two])).toEqual({ transcripts: [one, two], files: [log] });
  });

  it("keeps a 50 MiB cap", () => {
    expect(MAX_TRANSCRIPT_BYTES).toBe(50 * 1024 * 1024);
  });

  it("tells what is held from the types a browser shows during a drag, never from a name", () => {
    const item = (type: string) => ({ kind: "file", type }) as DataTransferItem;
    expect(willBeTranscript([item("application/vnd.orbyn.transcript+json")])).toBe(true);
    expect(willBeTranscript([item("")])).toBe(false);
    expect(willBeTranscript([item("application/vnd.orbyn.transcript+json"), item("text/plain")])).toBe(false);
    expect(willBeTranscript([])).toBe(false);
  });
});
