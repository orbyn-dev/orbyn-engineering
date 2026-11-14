import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

type Rgba = [number, number, number, number];
type Theme = "light" | "dark" | "dim";

const css = readFileSync(join(process.cwd(), "src/styles/tokens.css"), "utf8");

function block(selector: string): string {
  const start = css.indexOf(`\n${selector} {`);
  return css.slice(start, css.indexOf("\n}", start));
}

// Dim replaces only what differs from Dark, and Dark only what differs from Light.
const BLOCKS: Record<Theme, string[]> = {
  light: [block(":root")],
  dark: [block(':root[data-theme="dark"]'), block(":root")],
  dim: [block(':root[data-theme="dark"][data-preset="dim"]'), block(':root[data-theme="dark"]'), block(":root")],
};

function token(theme: Theme, name: string): Rgba {
  for (const text of BLOCKS[theme]) {
    const value = new RegExp(`${name}:\\s*([^;]+);`).exec(text)?.[1]?.trim();
    if (value !== undefined) return parse(value);
  }
  throw new Error(`${name} is not defined for ${theme}`);
}

function parse(value: string): Rgba {
  const hex = /^#([0-9a-f]{6})$/i.exec(value);
  if (hex?.[1]) return [0, 2, 4].map((i) => parseInt(hex[1]!.slice(i, i + 2), 16)).concat(1) as Rgba;
  const rgb = /^rgb\((\d+) (\d+) (\d+)(?: \/ ([\d.]+)%)?\)$/.exec(value);
  if (rgb) return [Number(rgb[1]), Number(rgb[2]), Number(rgb[3]), rgb[4] ? Number(rgb[4]) / 100 : 1];
  const hsl = /^hsl\(([\d.]+) ([\d.]+)% ([\d.]+)%(?: \/ ([\d.]+)%)?\)$/.exec(value);
  if (hsl) {
    const h = Number(hsl[1]);
    const s = Number(hsl[2]) / 100;
    const l = Number(hsl[3]) / 100;
    const a = s * Math.min(l, 1 - l);
    const channel = (n: number) => {
      const k = (n + h / 30) % 12;
      return 255 * (l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)));
    };
    return [channel(0), channel(8), channel(4), hsl[4] ? Number(hsl[4]) / 100 : 1];
  }
  throw new Error(`unsupported colour ${value}`);
}

const over = (top: Rgba, base: Rgba): Rgba => [0, 1, 2].map((i) => top[i]! * top[3] + base[i]! * (1 - top[3])).concat(1) as Rgba;

function luminance(c: Rgba): number {
  const [r, g, b] = c.slice(0, 3).map((x) => {
    const v = x / 255;
    return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
  }) as [number, number, number];
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function ratio(a: Rgba, b: Rgba): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x) as [number, number];
  return (hi + 0.05) / (lo + 0.05);
}

const PAGES = ["--color-background", "--color-sidebar"];
const TINTS = [null, "--color-sunken", "--color-rowHover", "--color-rowActive", "--color-selected", "--color-chip", "--color-message", "--color-warnSoft", "--color-dangerSoft", "--color-infoSoft"];

/** The lowest contrast of a foreground token over every surface it sits on: both page colours, each with every tint. */
function worst(theme: Theme, foreground: string): { min: number; on: string } {
  let min = Infinity;
  let on = "";
  for (const page of PAGES) {
    for (const tint of TINTS) {
      const surface = tint === null ? token(theme, page) : over(token(theme, tint), token(theme, page));
      const value = ratio(over(token(theme, foreground), surface), surface);
      if (value < min) [min, on] = [value, `${page}${tint === null ? "" : ` + ${tint}`}`];
    }
  }
  return { min, on };
}

describe("text contrast of the theme tokens (Orbyn designs v101 shared contrast tokens)", () => {
  const MUTED_MIN: Record<Theme, number> = { light: 4.95, dark: 6.25, dim: 6.79 };

  for (const theme of ["light", "dark", "dim"] as const) {
    it(`keeps muted text at ${MUTED_MIN[theme]}:1 or more on every surface it sits on in ${theme}`, () => {
      const { min, on } = worst(theme, "--color-muted");
      expect(min, `--color-muted on ${on}`).toBeGreaterThanOrEqual(MUTED_MIN[theme]);
    });
  }

  it("keeps amber text and red text at 4.5:1 or more on every surface in every theme", () => {
    for (const theme of ["light", "dark", "dim"] as const) {
      for (const foreground of ["--color-warnText", "--color-danger"]) {
        const { min, on } = worst(theme, foreground);
        expect(min, `${theme} ${foreground} on ${on}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("keeps the thread header's status text at 4.5:1 or more in every theme: the colour its own rules name, on every surface", () => {
    // The header's status ("Running", "Waiting for you", "Offline", "Archived", "Compacting") is a `.page-sub-item`, which takes the
    // colour of `.page-sub`; the waiting-for-a-limit label is `.limit-label`. Read the token each rule names, so changing a rule to a
    // weaker token fails here, not only changing a token's value.
    const colourOf = (file: string, selector: string) => {
      const text = readFileSync(join(process.cwd(), `src/styles/${file}`), "utf8");
      const start = text.indexOf(`\n${selector} {`);
      expect(start, `${selector} in ${file}`).toBeGreaterThanOrEqual(0);
      return /color:\s*var\((--[A-Za-z-]+)\)/.exec(text.slice(start, text.indexOf("\n}", start)))?.[1];
    };
    const tokens = { ".page-sub": colourOf("thread.css", ".page-sub"), ".limit-label": colourOf("detailsCard.css", ".limit-label"), ".conn-status": colourOf("browser.css", ".conn-status") };
    expect(tokens).toEqual({ ".page-sub": "--color-muted", ".limit-label": "--color-warnText", ".conn-status": "--color-muted" });
    for (const theme of ["light", "dark", "dim"] as const) {
      for (const [selector, foreground] of Object.entries(tokens)) {
        const { min, on } = worst(theme, foreground as string);
        expect(min, `${theme} ${selector} (${foreground}) on ${on}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });

  it("uses the designer's Light amber text and red", () => {
    expect(block(":root")).toContain("--color-warnText: #8f5004;");
    expect(block(":root")).toContain("--color-danger: #c22020;");
  });

  it("measures a known pair correctly", () => {
    expect(ratio([0, 0, 0, 1], [255, 255, 255, 1])).toBeCloseTo(21, 5);
    expect(worst("light", "--color-faint").min).toBeLessThan(4.5);
  });
});
