import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/shell.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const rule = (selector: string) => css.split("}").find((block) => block.trim().startsWith(`${selector} {`)) ?? "";

it("floats the row actions over the row's right edge with a fade, out of the title's way", () => {
  const actions = rule(".row-actions");
  expect(actions).toMatch(/position:\s*absolute/);
  expect(actions).toMatch(/opacity:\s*0/);
  // Hidden actions are out of the Tab order: visibility, delayed past the fade.
  expect(actions).toMatch(/visibility:\s*hidden/);
  expect(actions).toMatch(/visibility 0s linear var\(--m-fast\)/);
  expect(actions).toMatch(/linear-gradient\(to right, transparent, var\(--row-state\)/);
  // The thread row anchors them, and the trailing wrapper no longer does.
  expect(rule(".thread-row")).toMatch(/position:\s*relative/);
  expect(rule(".row-trailing")).not.toMatch(/position:/);
});

it("shows them on hover, selection and keyboard focus, not by taking the status text's place", () => {
  expect(css).toMatch(/li:hover > \.thread-row \.row-actions,\s*li\[data-selected\] \.thread-row \.row-actions,\s*\.thread-row:focus-visible \.row-actions,\s*\.thread-row:has\(:focus-visible\) \.row-actions \{\s*visibility: visible;\s*opacity: 1/);
  expect(css).not.toMatch(/\.row-rest \{\s*display: none/);
});

it("drops the project chip on a narrow row so the title keeps the room", () => {
  expect(rule(".thread-row")).toMatch(/container:\s*thread-row \/ inline-size/);
  expect(css).toMatch(/@container thread-row \(max-width: 240px\) \{\s*\.thread-row-project \{\s*display: none;/);
});

it("keeps a marked row's trailing end clear of the action strip, so a woke mark or pin is never under Snooze", () => {
  expect(css).toMatch(/\.thread-row:has\(\.row-woke, \.row-pin\) \.row-trailing \{\s*margin-right: 70px;/);
  expect(css).not.toMatch(/\.row-woke,\s*\.row-pin \{[^}]*z-index/);
});

it("does not keep the strip on the open row, and the drag and highlight fills feed the fade", () => {
  expect(css).not.toMatch(/aria-current="page"\] \.row-actions/);
  for (const selector of ["li[data-dragging] .thread-row", "li[data-highlight] .thread-row", "li[data-selected] .thread-row", ".thread-row:hover"]) {
    expect(rule(selector), selector).toMatch(/--row-state:/);
  }
});

it("shows the strip on keyboard focus, never on a mouse click's focus, and keeps iOS from its own callout", () => {
  // :focus-within would keep the strip after a click focused the row and the pointer left.
  expect(css).not.toMatch(/\.thread-row:focus-within \.row-actions/);
  expect(rule(".thread-row")).toMatch(/-webkit-touch-callout:\s*none/);
});
