import { readFileSync } from "node:fs";
import { expect, it } from "vitest";

const css = readFileSync("src/features/repos/repos.css", "utf8");

it("puts a Repos popover row's remove button on its first line, after the counts", () => {
  expect(css).toMatch(/\.repos-pop-row \{[^}]*grid-template-columns: minmax\(0, 1fr\) auto auto;/);
  expect(css).toMatch(/\.repos-pop-row > button \{ grid-row: 1; grid-column: 3; \}/);
});
