import { describe, expect, it } from "vitest";
import { attachmentSourceOf, fileOfTake, isSnapShotSource, shortcutKeys, shortcutText } from "./types";
import { sourceOf, takeOf } from "./snapshotFixture.test-support";

describe("the attachment source extension", () => {
  it("reads the service's optional source defensively", () => {
    expect(isSnapShotSource(sourceOf())).toBe(true);
    expect(isSnapShotSource(undefined)).toBe(false);
    expect(isSnapShotSource({ ...sourceOf(), kind: "file" })).toBe(false);
    expect(isSnapShotSource(sourceOf({ app_name: "a".repeat(255), window_title: "a".repeat(1000) }))).toBe(true);
    // The shell and the service count characters, not UTF-16 code units.
    expect(isSnapShotSource(sourceOf({ app_name: "😀".repeat(255), window_title: "😀".repeat(1000) }))).toBe(true);
    expect(isSnapShotSource({ ...sourceOf(), app_name: "a".repeat(256) })).toBe(false);
    expect(isSnapShotSource({ ...sourceOf(), window_title: "a".repeat(1001) })).toBe(false);
    expect(isSnapShotSource({ ...sourceOf(), app_name: "😀".repeat(256) })).toBe(false);
    expect(isSnapShotSource({ ...sourceOf(), window_title: "😀".repeat(1001) })).toBe(false);
    expect(isSnapShotSource({ ...sourceOf(), accessible_text: "a".repeat(32_001) })).toBe(false);
    expect(attachmentSourceOf({ id: "a", thread_id: "t", name: "x", mime: "image/png", size: 1n, created_at: "", source: sourceOf() })).toMatchObject({ app_name: "Safari" });
    expect(attachmentSourceOf(null)).toBeNull();
  });

  it("names the default chord Cmd+Shift+9 on macOS and Ctrl+Shift+9 on Windows", () => {
    const chord = { kind: "chord" as const, key: "9", mod: true, shift: true, alt: false, ctrl: false };
    expect(shortcutKeys(chord, "macos").map((key) => key.label)).toEqual(["⌘", "⇧", "9"]);
    expect(shortcutKeys(chord, "windows").map((key) => key.label)).toEqual(["Ctrl", "Shift", "9"]);
    expect(shortcutText(chord, "macos")).toBe("⌘⇧9");
    expect(shortcutText(chord, "windows")).toBe("Ctrl+Shift+9");
  });

  it("decodes a take into a PNG File without keeping the base64", async () => {
    const file = fileOfTake(takeOf({ png_base64: "aGVsbG8=" }));
    expect(file).toBeInstanceOf(File);
    expect(file.type).toBe("image/png");
    expect(await file.text()).toBe("hello");
  });
});
