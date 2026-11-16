import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const read = (file: string) => readFileSync(join(process.cwd(), file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const changes = read("src/styles/changes.css");
const pages = read("src/styles/pages.css");
const rule = (css: string, selector: string) => css.split("}").find((block) => block.trim().startsWith(`${selector} {`)) ?? "";

it("wraps long diff lines in the panel and keeps one number at the first row of a wrapped line", () => {
  expect(rule(changes, ".d2 .l")).toMatch(/white-space:\s*pre-wrap/);
  expect(rule(changes, ".d2 .l")).toMatch(/overflow-wrap:\s*anywhere/);
  expect(rule(changes, ".d2 .l")).toMatch(/grid-template-columns:\s*44px 14px minmax\(0, 1fr\)/);
  expect(rule(changes, '.d2[data-wrap="off"] .l')).toMatch(/white-space:\s*pre\b/);
});

it("turns the gutter number into a + under the pointer or the keyboard's line, and leaves the number where there is no hover", () => {
  expect(rule(changes, ".gut .gp")).toMatch(/display:\s*none/);
  expect(changes).toMatch(/\.l:hover \.gut \.gp,\s*\.l\.hov \.gut \.gp,\s*\.gut:focus-visible \.gp \{\s*display: grid/);
  expect(changes).toMatch(/@media \(hover: none\), \(pointer: coarse\) \{\s*\.gut \.gn \{\s*display: inline/);
});

it("scrolls long patch lines sideways on the pull request page", () => {
  expect(rule(pages, ".pull-patch")).toMatch(/min-width:\s*0/);
  expect(rule(pages, ".pull-patch")).toMatch(/overflow-x:\s*auto/);
});
