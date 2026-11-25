import { describe, expect, it } from "vitest";
import { mediaKind, rawUrl } from "./media";

const asked = (url: string) => new URL(url, "http://127.0.0.1");

describe("mediaKind", () => {
  it.each([
    ["a/b/Photo.JPG", "image"],
    ["logo.svg", "image"],
    ["clip.webm", "video"],
    ["song.flac", "audio"],
    ["doc.pdf", "pdf"],
    ["Inter.woff2", "font"],
    ["main.rs", null],
    ["png", null],
  ])("reads %s as %s", (path, kind) => expect(mediaKind(path)).toBe(kind));
});

describe("rawUrl", () => {
  it("sends no repo_id for the thread's own checkout, as it always has", () => {
    const url = asked(rawUrl("t1", "shots/a b.png"));
    expect(url.pathname).toBe("/api/threads/t1/files/raw");
    expect(url.searchParams.get("path")).toBe("shots/a b.png");
    expect(url.searchParams.has("repo_id")).toBe(false);
    expect(url.searchParams.has("download")).toBe(false);
  });

  it("names another repository only when one is given", () => {
    const url = asked(rawUrl("t1", "shots/a b.png", false, "planner"));
    expect(url.searchParams.get("path")).toBe("shots/a b.png");
    expect(url.searchParams.get("repo_id")).toBe("planner");
    expect(url.searchParams.has("download")).toBe(false);
  });

  it("keeps the repo_id on the download the bar's Save file uses", () => {
    const url = asked(rawUrl("t1", "shots/a b.png", true, "planner"));
    expect(url.searchParams.get("repo_id")).toBe("planner");
    expect(url.searchParams.get("download")).toBe("true");
  });
});
