import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const tokens = readFileSync(join(process.cwd(), "src/styles/tokens.css"), "utf8");
const pulls = readFileSync(join(process.cwd(), "src/features/pulls/pulls.css"), "utf8");

/** Relative luminance of an oklch colour, clipped to sRGB. */
function luminance(l: number, c: number, hue: number): number {
  const h = (hue * Math.PI) / 180;
  const a = c * Math.cos(h);
  const b = c * Math.sin(h);
  const l3 = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m3 = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s3 = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  const clip = (v: number) => Math.min(1, Math.max(0, v));
  return (
    0.2126 * clip(4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3) +
    0.7152 * clip(-1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3) +
    0.0722 * clip(-0.0041960863 * l3 - 0.7034186147 * m3 + 1.707614701 * s3)
  );
}

const ratio = (a: number, b: number) => (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
const INK = 0.0030; // #0a0a0a
const WHITE = 1;

function fills(from: number): Map<string, number> {
  const out = new Map<string, number>();
  for (const line of tokens.slice(from, tokens.indexOf("\n}", from)).split("\n")) {
    const m = /--project-color-(\w+): oklch\(([\d.]+)% ([\d.]+) ([\d.]+)\)/.exec(line);
    if (m) out.set(m[1]!, luminance(Number(m[2]) / 100, Number(m[3]), Number(m[4])));
  }
  return out;
}

const first = tokens.indexOf("--project-color-gray");
const LIGHT = fills(first);
const DARK = fills(tokens.indexOf("--project-color-gray", first + 5));
const lightWhite = new Set([...pulls.matchAll(/:root:not\(\[data-theme="dark"\]\) \.project-color-(\w+)/g)].map((m) => m[1]!));

describe("avatar initials contrast (designs v103)", () => {
  it("reads 18 fills per theme", () => {
    expect(LIGHT.size).toBe(18);
    expect(DARK.size).toBe(18);
  });

  it("keeps Light initials at 4.5:1 or more on every fill", () => {
    for (const [name, y] of LIGHT) {
      const value = ratio(y, lightWhite.has(name) ? WHITE : INK);
      expect(value, `light ${name}`).toBeGreaterThanOrEqual(4.5);
    }
  });

  it("keeps Dark and Dim initials near-black at 6:1 or more on every fill", () => {
    for (const [name, y] of DARK) expect(ratio(y, INK), `dark ${name}`).toBeGreaterThanOrEqual(6);
  });
});
