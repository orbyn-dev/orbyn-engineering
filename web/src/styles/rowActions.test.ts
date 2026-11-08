import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/shell.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const rule = (selector: string) => css.split("}").find((block) => block.trim().startsWith(`${selector} {`)) ?? "";

it("floats the row actions over the row's right edge with a fade, out of the title's way", () => {
  const actions = rule(".row-actions");
  expect(actions).toMatch(/position:\s*absolute/);
  expect(actions).toMatch(/opacity:\s*0/);
  expect(actions).toMatch(/linear-gradient\(to right, transparent, var\(--row-state\)/);
  // The thread row anchors them, and the trailing wrapper no longer does.
  expect(rule(".thread-row")).toMatch(/position:\s*relative/);
  expect(rule(".row-trailing")).not.toMatch(/position:/);
});

it("shows them on hover, selection and keyboard focus, not by taking the status text's place", () => {
  expect(css).toMatch(/li:hover > \.thread-row \.row-actions,\s*li\[data-selected\] \.thread-row \.row-actions,\s*\.thread-row:focus-within \.row-actions \{\s*opacity: 1/);
  expect(css).not.toMatch(/\.row-rest \{\s*display: none/);
});
