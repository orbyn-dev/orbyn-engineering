import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const read = (file: string) => readFileSync(join(process.cwd(), file), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const thread = read("src/styles/thread.css");
const pages = read("src/styles/pages.css");
const rule = (css: string, selector: string) => css.split("}").find((block) => block.trim().startsWith(`${selector} {`)) ?? "";

it("shows the diff comment button on hover, focus and selection only, and keeps it in view beside a long line", () => {
  const button = rule(thread, ".patch-line-number");
  expect(button).toMatch(/opacity:\s*0/);
  expect(button).toMatch(/position:\s*sticky/);
  expect(thread).toMatch(/\.patch-line:hover \.patch-line-number,\s*\.patch-line\[data-selected\] \.patch-line-number,\s*\.patch-line-number:focus-visible,\s*\.patch-line-number\[aria-pressed="true"\] \{\s*opacity: 1/);
});

it("scrolls long diff lines sideways instead of cutting them, in the panel and on the pull request page", () => {
  expect(rule(thread, ".patch")).toMatch(/overflow-x:\s*auto/);
  expect(rule(thread, ".patch-inner")).toMatch(/min-width:\s*max-content/);
  expect(rule(thread, ".patch-line")).toMatch(/white-space:\s*pre/);
  expect(rule(pages, ".pull-patch")).toMatch(/min-width:\s*0/);
  expect(rule(pages, ".pull-patch")).toMatch(/overflow-x:\s*auto/);
});
