import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Issue #1124: a macOS-only chord or label must never reach a Windows screen. The Mac modifier glyphs are written
 * in one place — `formatShortcut`, and `modKeyLabel` for the sidebar — or chosen on the line itself by the client's
 * or the service's platform. Any other literal would render for everyone, so this guard fails on one.
 */
const MAC_MODIFIER = /[⌘⌥⌃⇧]/;

/** Lines that start a comment are not shown; only a comment on its own line is skipped. */
const COMMENT = /^\s*(?:\/\/|\/\*|\*)/;

/** A line that tests the platform. */
const TESTS_PLATFORM = /navigator\.platform|clientPlatform\(|isMac(?:Client|Platform)?\b|platform === "macos"/;

/**
 * A line that picks its text by platform cannot show a Mac glyph to Windows, but only when the glyph sits in the Mac branch:
 * after the test's `?` and before the branch's ` : `. A glyph in the other branch, or on a line that only mentions the platform, is shown to Windows.
 */
function picksPlatform(line: string): boolean {
  const test = TESTS_PLATFORM.exec(line);
  if (!test) return false;
  const question = line.indexOf("?", test.index);
  if (question === -1) return false;
  const otherBranch = line.indexOf(" : ", question);
  return otherBranch === -1 || !MAC_MODIFIER.test(line.slice(otherBranch));
}

/** The files where a Mac modifier glyph is written literally. */
const LITERAL_ALLOWED = new Set([
  // formatShortcut writes the Mac glyphs; nothing else may.
  "product/shell/shortcuts.ts",
  // modKeyLabel is the sidebar's copy of them.
  "features/workspace/sidebar/threadUndo.ts",
  // keyCaps takes formatShortcut's glyphs apart again to draw the key caps.
  "features/settings/pages/Keybindings.tsx",
  // The desktop app's quit overlay reads the service's macOS platform and never draws off it.
  "features/workspace/app-chrome/QuitConfirmation.tsx",
  // The quit rows, behind the service's macOS platform.
  "features/settings/sections/GeneralSection.tsx",
]);

interface SourceFile {
  /** Relative to web/src, with forward slashes. */
  path: string;
  text: string;
}

/** Every literal Mac modifier glyph that no helper, allowlisted file or platform choice covers, as `path:line: text`. */
function unguardedMacGlyphs(files: readonly SourceFile[]): string[] {
  return files
    .filter((file) => !/\.test\.tsx?$/.test(file.path) && !LITERAL_ALLOWED.has(file.path))
    .flatMap((file) =>
      file.text.split("\n").flatMap((line, at) =>
        MAC_MODIFIER.test(line) && !COMMENT.test(line) && !picksPlatform(line) ? [`${file.path}:${at + 1}: ${line.trim()}`] : [],
      ),
    );
}

const SOURCE_ROOT = join(import.meta.dirname, "..", "..");

/** Every TypeScript and stylesheet source file under web/src. */
function sourceFiles(dir: string): SourceFile[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return sourceFiles(path);
    if (!/\.(?:ts|tsx|css)$/.test(entry.name)) return [];
    const file = { path: relative(SOURCE_ROOT, path).replaceAll("\\", "/"), text: readFileSync(path, "utf8") };
    return [file];
  });
}

// Read once, before the tests: walking every source file is slow on a busy machine, and no test timeout covers it.
const SOURCES = sourceFiles(SOURCE_ROOT);

describe("macOS-only labels", () => {
  it("fails a Mac modifier glyph that nothing chooses by platform", () => {
    const files: SourceFile[] = [
      { path: "features/x/Window.tsx", text: '<button title="Save ⌘S">Save</button>' },
      { path: "features/x/Platform.tsx", text: '<button title={isMacClient() ? "Save ⌘S" : "Save Ctrl+S"}>Save</button>' },
      { path: "features/x/Reversed.tsx", text: '<button title={isMacClient() ? "Save Ctrl+S" : "Save ⌘S"}>Save</button>' },
      { path: "features/x/Note.tsx", text: "// A ⌘S in a comment is never drawn." },
      { path: "features/workspace/app-chrome/QuitConfirmation.tsx", text: "<b>Press ⌘Q again to quit</b>" },
    ];
    expect(unguardedMacGlyphs(files)).toEqual([
      'features/x/Window.tsx:1: <button title="Save ⌘S">Save</button>',
      'features/x/Reversed.tsx:1: <button title={isMacClient() ? "Save Ctrl+S" : "Save ⌘S"}>Save</button>',
    ]);
  });

  it("keeps every literal Mac modifier glyph in a helper, a macOS-only surface or a platform choice", () => {
    expect(unguardedMacGlyphs(SOURCES)).toEqual([]);
  });
});
