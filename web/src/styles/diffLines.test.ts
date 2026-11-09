import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const read = (file: string) => readFileSync(join(process.cwd(), file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const thread = [read("src/styles/composer.css"), read("src/styles/thread-panels.css")].join("\n");
const pages = read("src/styles/pages.css");
const rule = (css: string, selector: string) => css.split("}").find((block) => block.trim().startsWith(`${selector} {`)) ?? "";

it("keeps the diff line number visible and fades only the comment icon, always showing it where there is no hover", () => {
  const button = rule(thread, ".patch-line-number");
  expect(button).toMatch(/position:\s*sticky/);
  expect(button).not.toMatch(/opacity/);
  expect(rule(thread, ".patch-line-number > svg")).toMatch(/opacity:\s*0/);
  expect(thread).toMatch(/\.patch-line:hover \.patch-line-number > svg,\s*\.patch-line\[data-selected\] \.patch-line-number > svg,\s*\.patch-line-number:focus-visible > svg,\s*\.patch-line-number\[aria-pressed="true"\] > svg \{\s*opacity: 1/);
  expect(thread).toMatch(/@media \(hover: none\), \(pointer: coarse\) \{\s*\.patch-line-number > svg \{\s*opacity: 1/);
});

it("scrolls long diff lines sideways instead of cutting them, in the panel and on the pull request page", () => {
  expect(rule(thread, ".patch")).toMatch(/overflow-x:\s*auto/);
  expect(rule(thread, ".patch-inner")).toMatch(/min-width:\s*max-content/);
  expect(rule(thread, ".patch-line")).toMatch(/white-space:\s*pre/);
  expect(rule(pages, ".pull-patch")).toMatch(/min-width:\s*0/);
  expect(rule(pages, ".pull-patch")).toMatch(/overflow-x:\s*auto/);
});
