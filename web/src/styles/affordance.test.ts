import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

/**
 * jsdom has no layout and no hover, so the three surfaces named by #1116 are read from their stylesheets: the composer
 * controls, the heading's project switcher and the Providers rows must each carry a pointer cursor, the designed raised
 * hover fill and the designed focus ring, and nothing may take them that is not a control.
 *
 * Values come from the Engineer Designs: `--raise` is `rgba(0,0,0,.035)` and `--t4` is `rgba(0,0,0,.44)`, which are this
 * app's `--color-hover` and `--color-faint`; `.tcr:hover { background: var(--raise) }`, `.tcr .chev { color: var(--t4) }`
 * and `.focusring { outline: 2px solid var(--t1); outline-offset: 2px }` (`--t1` is `--color-focus`).
 */
const strip = (css: string) => css.replace(/\/\*[\s\S]*?\*\//g, "");
const sheet = (path: string) => strip(readFileSync(join(process.cwd(), path), "utf8"));

/** The declaration body of the rule whose selector list names `selector` exactly. */
function rule(css: string, selector: string): string {
  for (const [, selectors = "", body = ""] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (selectors.split(",").some((part) => part.trim() === selector)) return body;
  }
  return "";
}

const modelPicker = sheet("src/styles/model-picker.css");
const composer = sheet("src/styles/composer.css");
const providers = sheet("src/features/settings/ProviderInstances.css");
const base = sheet("src/styles/base.css");

it("gives the hover fill and the pointer cursor to composer controls only, never to a value shown in a span", () => {
  // A `.composer-control` that is not a button shows a value ("No engine", a model name): it must not pretend to be one.
  expect(rule(modelPicker, ".composer-control")).not.toMatch(/cursor:/);
  expect(rule(modelPicker, ".composer-control")).not.toMatch(/:hover/);
  expect(rule(modelPicker, "button.composer-control")).toMatch(/cursor:\s*pointer/);
  expect(rule(modelPicker, "button.composer-control:hover:not(:disabled)")).toMatch(/background:\s*var\(--color-hover\)/);
});

it("keeps the designed focus ring on every composer control, from the one global rule", () => {
  // The designs' `.focusring` is `outline: 2px solid var(--t1); outline-offset: 2px`.
  expect(rule(base, "button:focus-visible")).toMatch(/outline:\s*2px solid var\(--color-focus\)/);
  expect(rule(base, "button:focus-visible")).toMatch(/outline-offset:\s*2px/);
  // No composer rule may take the ring back off a control that can be tabbed to.
  for (const css of [modelPicker, composer]) {
    for (const [, selector = "", body = ""] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
      if (!/composer-control|hero-project|hero-link/.test(selector)) continue;
      expect(body, selector.trim()).not.toMatch(/outline:\s*(none|0)/);
    }
  }
});

it("marks the heading's project name as the project switcher: pointer cursor and the raised hover fill", () => {
  expect(rule(composer, ".hero-project")).toMatch(/cursor:\s*pointer/);
  expect(rule(composer, ".hero-project")).toMatch(/display:\s*inline-flex/);
  expect(rule(composer, ".hero-project")).toMatch(/gap:\s*4px/);
  expect(rule(composer, ".hero-project:hover:not(:disabled)")).toMatch(/background:\s*var\(--color-hover\)/);
  // The link that starts without a project is the same kind of control and takes the same fill.
  expect(rule(modelPicker, ".hero-link")).toMatch(/cursor:\s*pointer/);
  expect(rule(modelPicker, ".hero-link:hover:not(:disabled)")).toMatch(/background:\s*var\(--color-hover\)/);
});

it("cues an openable Providers row with the designed row hover and a trailing chevron at the faint tone", () => {
  // The design's disclosure row: `.tcr { }` is pointer-clickable, `.tcr:hover { background: var(--raise) }` and
  // `.tcr .chev { color: var(--t4) }` sits at the end of the row.
  expect(rule(providers, ".set-tr-open")).toMatch(/cursor:\s*pointer/);
  expect(rule(providers, ".set-tr-open:hover")).toMatch(/background:\s*var\(--color-hover\)/);
  expect(rule(providers, ".set-row-open > svg")).toMatch(/color:\s*var\(--color-faint\)/);
  // The name button is the row's real control: it keeps its own ring when tabbed to.
  expect(rule(providers, ".set-row-open:focus-visible")).toMatch(/outline:\s*2px solid var\(--color-focus\)/);
});
