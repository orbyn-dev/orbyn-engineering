import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

const css = readFileSync("src/features/library/library.css", "utf8");

it("switches the Library to two columns by the panel's width, never the window's", () => {
  expect(css).toMatch(/\.lib \{[^}]*container: lib \/ inline-size;/);
  expect(css).toMatch(/@container lib \(min-width: 676px\) \{ \.lib-body \{ grid-template-columns: minmax\(0, 2fr\) minmax\(0, 3fr\); \}/);
  expect(css).not.toMatch(/@media \(min-width: 700px\) \{ \.lib-body/);
});
