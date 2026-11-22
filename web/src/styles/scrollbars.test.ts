import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

const stylesDir = join(process.cwd(), "src/styles");
const stylesheet = (name: string) => readFileSync(join(stylesDir, name), "utf8");

function stylesheets(folder: string): string[] {
  return readdirSync(folder, { withFileTypes: true }).flatMap((entry) => {
    const path = join(folder, entry.name);
    if (entry.isDirectory()) return stylesheets(path);
    return entry.isFile() && entry.name.endsWith(".css") ? [path] : [];
  });
}

const allCss = Object.fromEntries(stylesheets(join(process.cwd(), "src")).map((path) => [
  relative(join(process.cwd(), "src"), path).replaceAll("\\", "/"),
  readFileSync(path, "utf8"),
]));

const escape = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/** Every body declared for one selector, so a selector repeated in another block is checked too. */
function bodies(source: string, selector: string): string[] {
  const pattern = new RegExp(`(?:^|\\n)[ \\t]*${escape(selector)}\\s*\\{([^}]*)\\}`, "g");
  return Array.from(source.matchAll(pattern), ([, body = ""]) => body);
}

function values(source: string, selector: string, property: string): string[] {
  return bodies(source, selector).flatMap((body) =>
    Array.from(body.matchAll(new RegExp(`(?<![\\w-])${escape(property)}:\\s*([^;]+);`, "g")), ([, value = ""]) => value.trim()),
  );
}

const base = stylesheet("base.css");
const tokens = stylesheet("tokens.css");

describe("scrollbars", () => {
  /**
   * Orbyn Engineer Designs, model-picker frames: the scrollbar is a 4px thumb in --line2, radius 2px,
   * on a clear track. Every value below comes from that frame, not from a platform default.
   */
  it("paints the designed thumb in Chromium and WebKit", () => {
    expect(bodies(base, "::-webkit-scrollbar").map((body) => body.trim())).toContain("width: 4px;\n  height: 4px;");
    expect(values(base, "::-webkit-scrollbar-thumb", "width")).toEqual([]);
    expect(values(base, "::-webkit-scrollbar-thumb", "background")).toEqual(["var(--color-borderStrong)"]);
    expect(values(base, "::-webkit-scrollbar-thumb", "border-radius")).toEqual(["2px"]);
    expect(values(base, "::-webkit-scrollbar-track", "background")).toEqual(["transparent"]);
    expect(values(base, "::-webkit-scrollbar-corner", "background")).toEqual(["transparent"]);
  });

  /**
   * Firefox paints no scrollbar pseudo-elements, so it needs the standard pair; Chromium honours the
   * standard pair over the pseudo-elements, so the pair is scoped to the engines without them.
   */
  it("themes Firefox without overriding the Chromium thumb", () => {
    const scoped = /@supports not selector\(::-webkit-scrollbar\) \{([\s\S]*?)\n\}/.exec(base)?.[1] ?? "";
    expect(scoped, "the standard pair must be scoped so Chromium keeps the 4px thumb").toContain("scrollbar-width: thin;");
    expect(scoped).toContain("scrollbar-color: var(--color-borderStrong) transparent;");
    expect(scoped).toContain("  * {");
    // Nothing outside that block sets the standard pair, or Chromium would drop the designed thumb.
    for (const [name, text] of Object.entries(allCss)) {
      if (name.endsWith("base.css")) continue;
      expect(text.match(/scrollbar-color:/g), name).toBeNull();
      for (const value of Array.from(text.matchAll(/scrollbar-width:\s*([^;]+);/g), ([, value = ""]) => value.trim())) {
        expect(value, `${name} may only hide a chip row's bar; the width comes from base.css`).toBe("none");
      }
    }
  });

  /** --line2 in the designs: rgba(0, 0, 0, .12) in Light and rgba(255, 255, 255, .14) in Dark and Dim. */
  it("follows the theme in Light, Dark and Dim", () => {
    const light = values(tokens, ":root", "--color-borderStrong");
    const dark = values(tokens, ':root[data-theme="dark"]', "--color-borderStrong");
    const dim = values(tokens, ':root[data-theme="dark"][data-preset="dim"]', "--color-borderStrong");
    expect(light).toHaveLength(1);
    expect(dark).toHaveLength(1);
    expect(dim, "Dim must state the thumb colour it inherits, or the scrollbar would fall back to the platform").toHaveLength(1);
    expect(light[0]).toBe("rgb(0 0 0 / 12%)");
    expect(dark[0]).toBe("rgb(255 255 255 / 14%)");
    expect(dim[0]).toBe(dark[0]);
  });
});
