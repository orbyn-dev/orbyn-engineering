import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";
import { MOTION } from "../lib/motion";

const dir = join(process.cwd(), "src/styles");
const files = readdirSync(dir).filter((name) => name.endsWith(".css"));
const css = Object.fromEntries(files.map((name) => [name, readFileSync(join(dir, name), "utf8")]));

const srcDir = join(process.cwd(), "src");

function stylesheets(folder: string): string[] {
  return readdirSync(folder, { withFileTypes: true }).flatMap((entry) => {
    const path = join(folder, entry.name);
    if (entry.isDirectory()) return stylesheets(path);
    return entry.isFile() && entry.name.endsWith(".css") ? [path] : [];
  });
}

// Every stylesheet under src, so CSS imported by feature modules is checked too. Keys are paths from src.
const allCss = Object.fromEntries(stylesheets(srcDir).map((path) => [
  relative(srcDir, path).replaceAll("\\", "/"), readFileSync(path, "utf8"),
]));

// Sizes and radii the internal designs use; 10.5, 13.5, 14.5 and 15 and the 5–20 radii come from their source.
const SCALE = new Set(["8", "10.5", "11", "11.5", "12", "12.5", "13", "13.5", "14", "14.5", "15", "16", "18", "20", "22", "26", "30", "36"]);
const RADII = new Set(["2", "4", "5", "6", "7", "8", "9", "10", "12", "14", "16", "20", "999"]);

/**
 * Badge glyphs drawn below the 11px floor, by selector. They are decorative letters inside a
 * fixed-size badge, not reading text; everything that reads as text stays on the scale.
 */
const GLYPH_SIZES: Record<string, string> = {
  ".pull-av": "avatar initials, 20 px circle",
  ".welcome-avatar": "project letter tile, 18 px",
  ".set-mark": "project letter tile, 18 px",
};

/** The font-size declarations the scale applies to: those outside the allow-listed glyph selectors. */
function textFontSizes(text: string): string {
  const bare = text.replace(/\/\*[\s\S]*?\*\//g, "");
  return Array.from(bare.matchAll(/([^{}]+)\{([^{}]*)\}/g))
    .filter(([, selector = ""]) => !selector.split(",").every((part) => part.trim() in GLYPH_SIZES))
    .map(([, , body = ""]) => body)
    .join("\n");
}

function violations(text: string, pattern: RegExp, allowed: Set<string>, name: string): string[] {
  return Array.from(text.matchAll(pattern), ([declaration, value]) =>
    value !== undefined && allowed.has(value) ? null : `${name}: ${declaration}`,
  ).filter((violation): violation is string => violation !== null);
}

/** Every pixel value in a border-radius, so a shorthand or a "/" form cannot hide an off-set corner. */
function radiusViolations(text: string, name: string): string[] {
  return Array.from(text.matchAll(/border-radius:\s*([^;{}]+)/g)).flatMap(([, value]) =>
    violations(value ?? "", /([\d.]+)px/g, RADII, name),
  );
}

describe("design tokens", () => {
  it("keeps literal colours in the token owner only", () => {
    for (const [name, text] of Object.entries(css)) {
      if (name === "tokens.css") continue;
      expect(text.match(/#[0-9a-f]{3,8}\b/gi), name).toBeNull();
    }
  });

  it("keeps the faint step off text: only placeholders, disabled controls, icons and dividers use it for color", () => {
    // Selectors where the faint step is allowed: placeholder-like search fields, dimmed or disabled controls, icons and dividers.
    const exempt = /::placeholder|:disabled|\[data-dim\]|\.icon-btn|> svg|\.crumb-slash|\.search-trigger|\.search-field|^\.set-find$|^\.settings-search$/;
    const all = (folder: string): string[] => readdirSync(folder, { withFileTypes: true }).flatMap((entry) =>
      entry.isDirectory() ? all(join(folder, entry.name)) : entry.name.endsWith(".css") ? [join(folder, entry.name)] : []);
    const offenders: string[] = [];
    for (const file of all(join(process.cwd(), "src"))) {
      if (file.endsWith("tokens.css")) continue;
      const text = readFileSync(file, "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
      for (const [, selector = "", body = ""] of text.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
        if (!/(?<![-\w])color:\s*[^;]*--color-faint/.test(body)) continue;
        if (selector.split(",").every((part) => exempt.test(part.trim()))) continue;
        offenders.push(`${file.replace(`${process.cwd()}/`, "")}: ${selector.trim().replace(/\s+/g, " ")}`);
      }
    }
    expect(offenders).toEqual([]);
  });

  it("keeps font sizes on the type scale", () => {
    for (const [name, text] of Object.entries(allCss)) {
      expect(violations(textFontSizes(text), /font-size:\s*([\d.]+)px/g, SCALE, name)).toEqual([]);
      expect(violations(text, /--(?:text-[a-z]+|control-fs(?:-sm)?|button-fs):\s*([\d.]+)px/g, SCALE, name)).toEqual([]);
    }
  });

  it("allows a size under 11px only on a named badge glyph, with its reason", () => {
    for (const [selector, reason] of Object.entries(GLYPH_SIZES)) expect(reason, selector).not.toBe("");
    const css = (selector: string) => `${selector} { font-size: 10px; }`;
    expect(violations(textFontSizes(css(".pull-av")), /font-size:\s*([\d.]+)px/g, SCALE, "x")).toEqual([]);
    expect(violations(textFontSizes(css(".caption, .pull-av")), /font-size:\s*([\d.]+)px/g, SCALE, "x")).toEqual(["x: font-size: 10px"]);
    expect(violations(textFontSizes(css(".some-label")), /font-size:\s*([\d.]+)px/g, SCALE, "x")).toEqual(["x: font-size: 10px"]);
  });

  it("keeps every glyph exception in use", () => {
    const all = Object.values(allCss).join("\n");
    for (const selector of Object.keys(GLYPH_SIZES)) expect(all, selector).toContain(`${selector} {`);
  });

  it("keeps corners on the radius set", () => {
    for (const [name, text] of Object.entries(allCss)) {
      expect(radiusViolations(text, name)).toEqual([]);
    }
  });

  it("reports off-scale values in feature stylesheets", () => {
    const name = "features/settings/ScheduledTasks.css";
    expect(Object.keys(allCss)).toContain(name);
    const text = ".example { font-size: 17px; border-radius: var(--radius-sm) 3px / 0 50%; }";
    expect(violations(text, /font-size:\s*([\d.]+)px/g, SCALE, name)).toEqual([
      `${name}: font-size: 17px`,
    ]);
    expect(radiusViolations(text, name)).toEqual([`${name}: 3px`]);
  });

  it("mirrors the motion durations in the timers and keeps the panel tokens on them", () => {
    const tokens = css["tokens.css"] ?? "";
    const ms = (name: string) => Number(new RegExp(`--${name}: calc\\((\\d+)ms`).exec(tokens)?.[1]);
    expect(MOTION).toEqual({ fast: ms("m-fast"), baseOut: ms("m-base-out"), slow: ms("m-slow"), slowOut: ms("m-slow-out"), wait: ms("m-wait") });
    expect(tokens).toContain("--ease-drawer: var(--m-out);");
    expect(tokens).toContain("--panel-duration: var(--m-slow);");
  });

  it("runs one loop only: the spinner, 900ms linear", () => {
    const loops = Object.values(css).flatMap((text) => text.match(/animation:[^;]*\binfinite\b[^;]*;/g) ?? []);
    expect(loops).toEqual(["animation: spin 900ms linear infinite;"]);
  });

  it("keeps the hold-to-quit fill linear under reduced motion", () => {
    const shell = css["shell.css"] ?? "";
    expect(shell).toMatch(/\.quit-progress\[data-holding\]::after \{\s*width: 100%;\s*transition: width var\(--hold-ms\) linear;/);
    const reduced = shell.slice(shell.indexOf("@media (prefers-reduced-motion: reduce)", shell.indexOf(".quit-progress")));
    expect(reduced).toContain("transition-timing-function: linear !important;");
  });
});

describe("control radii (Orbyn Components)", () => {
  const block = (text: string, selector: string) => new RegExp(`(?:^|\\n)${selector.replace(/[.[\]]/g, "\\$&")}\\s*\\{([^}]*)\\}`).exec(text)?.[1] ?? "";

  it("gives buttons 9px, every segmented control 10px with 7px segments, menus 12px and fields 10px", () => {
    expect(block(css["base.css"]!, ".btn")).toContain("border-radius: var(--radius-9)");
    expect(block(css["pages.css"]!, ".segmented")).toContain("border-radius: var(--radius-10)");
    expect(block(css["pages.css"]!, ".segmented button")).toContain("border-radius: var(--radius-7)");
    expect(block(css["pages.css"]!, ".segment")).toContain("border-radius: var(--radius-10)");
    expect(block(css["pages.css"]!, ".segment button")).toContain("border-radius: var(--radius-7)");
    expect(block(css["thread.css"]!, ".access-segment")).toContain("border-radius: var(--radius-10)");
    expect(block(css["thread.css"]!, ".access-segment button")).toContain("border-radius: var(--radius-7)");
    expect(block(css["shell.css"]!, ".route-tabs button")).toContain("border-radius: var(--radius-md)");
    expect(block(css["base.css"]!, ".menu")).toContain("border-radius: var(--radius-lg)");
  });

  it("has no settings-only radius override for them", () => {
    const settings = readFileSync(join(process.cwd(), "src/features/settings/settingsDesign.css"), "utf8");
    expect(settings).not.toMatch(/\.segmented[^{]*\{[^}]*border-radius/);
    expect(settings).not.toMatch(/\.btn[^{]*\{[^}]*border-radius/);
    expect(css["pages.css"]).not.toMatch(/\.settings-(?:main|scope) \.btn\s*\{[^}]*border-radius/);
  });
});
