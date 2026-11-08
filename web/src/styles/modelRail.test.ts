import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/thread.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const px = (selector: string, property: string) => Number(new RegExp(`${selector.replace(/[.[\]]/g, "\\$&")} \\{[^}]*?\\b${property}:\\s*(\\d+)px`).exec(css)?.[1]);

it("fits the two-letter engine glyph inside its tab and the tab inside the rail", () => {
  const glyph = px(".model-tab .engine-glyph", "width");
  expect(glyph).toBeGreaterThanOrEqual(26);
  expect(px(".model-tab", "width")).toBeGreaterThanOrEqual(glyph + 8);
  // The rail holds the tab between its padding.
  expect(px(".model-rail", "width")).toBeGreaterThanOrEqual(px(".model-tab", "width") + 8);
});

it("insets the side chat's empty state 16px like the other tabs", () => {
  expect(/\.side-chat-pane > \.empty \{[^}]*padding:\s*12px 16px/.test(css)).toBe(true);
});
