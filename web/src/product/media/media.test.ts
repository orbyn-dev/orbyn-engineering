import { describe, expect, it } from "vitest";
import { mediaKind } from "./media";

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
