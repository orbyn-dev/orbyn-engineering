import { readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

/** jsdom has no layout, so the Providers table's stylesheet is read: its grid must lose Version, never Status. */
const raw = readFileSync(join(process.cwd(), "src/features/settings/settingsDesign.css"), "utf8");
const css = raw.replace(/\/\*[\s\S]*?\*\//g, "");

/** The declaration body of the first rule whose selector list names `selector`. */
const rule = (selector: string) => {
  for (const [, selectors = "", body = ""] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (selectors.split(",").some((part) => part.trim() === selector)) return body;
  }
  return "";
};

/** The body of a container query, with nested braces matched. */
function container(maxWidth: number): string {
  const open = new RegExp(`@container\\s*\\(max-width:\\s*${maxWidth}px\\)\\s*\\{`).exec(css);
  if (!open) return "";
  const start = open.index + open[0].length;
  let depth = 1;
  for (let index = start; index < css.length; index += 1) {
    if (css[index] === "{") depth += 1;
    if (css[index] === "}") depth -= 1;
    if (depth === 0) return css.slice(start, index);
  }
  return "";
}

/** A grid template's top-level tracks, keeping `minmax(120px, 1fr)` together. */
function tracks(template: string): string[] {
  const found: string[] = [];
  let depth = 0;
  let track = "";
  for (const character of template.trim()) {
    if (character === "(") depth += 1;
    if (character === ")") depth -= 1;
    if (/\s/.test(character) && depth === 0) {
      if (track) found.push(track);
      track = "";
    } else {
      track += character;
    }
  }
  if (track) found.push(track);
  return found;
}

const templateOf = (text: string) => /--prov-columns:\s*([^;]+);/.exec(text)?.[1]?.trim() ?? "";

it("keeps the six-column Providers template by default, with a readable name column and the 90px Version", () => {
  expect(rule(".set-tbl.prov")).toMatch(/container-type:\s*inline-size/);
  expect(rule(".set-tbl.prov .set-tr")).toMatch(/grid-template-columns:\s*var\(--prov-columns\)/);
  expect(tracks(templateOf(rule(".set-tbl.prov")))).toEqual([
    "28px", "minmax(120px, 1fr)", "90px", "130px", "60px", "88px",
  ]);
});

it("hides the Version cell and drops its 90px column below 800px, keeping Status and In use", () => {
  const narrow = container(799);
  expect(narrow).toMatch(/\.set-tbl\.prov \.set-tr > :nth-child\(3\)\s*\{\s*display:\s*none;/);
  const template = templateOf(narrow);
  expect(template).not.toBe("");
  expect(template).not.toContain("90px");
  expect(tracks(template)).toEqual([
    "28px", "minmax(120px, 1fr)", "130px", "60px", "88px",
  ]);
});

it("keeps Version on every wide window: the settings body caps the table at 864px, above the 800px threshold", () => {
  // 920px body minus 28px padding on each side; the threshold must sit below it or Version never shows.
  const widest = 920 - 2 * 28;
  const threshold = Number(/@container\s*\(max-width:\s*(\d+)px\)/.exec(css)?.[1]) + 1;
  expect(threshold).toBe(800);
  expect(threshold).toBeLessThanOrEqual(widest);
});

it("lets the Status cell wrap and never squeezes it below its minimum", () => {
  expect(rule(".set-tbl.prov .set-states")).toMatch(/min-width:\s*0/);
  expect(rule(".set-tbl.prov .set-states")).toMatch(/white-space:\s*normal/);
  expect(rule(".set-tbl.prov .set-states")).not.toMatch(/white-space:\s*nowrap/);
  // The budget at 528px is in the stylesheet: 28 + 60 + 88 fixed + 4 gaps of 12 + 8 padding
  // leaves the name 166px and Status 130px, both at least 120px. The template keeps the name at 120px.
  const budget = /Budget at 528, 628, 852px:([^*]+)/.exec(raw);
  expect(budget, "the stylesheet states the 528/628/852 budget").not.toBeNull();
  expect(budget?.[1]).toMatch(/528[^;]*166/);
  expect(budget?.[1]).toMatch(/628[^;]*266/);
  expect(budget?.[1]).toMatch(/852[^;]*490/);
});
