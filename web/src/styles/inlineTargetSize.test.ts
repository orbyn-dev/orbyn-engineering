import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Target size (WCAG 2.2 AA 2.5.8): the inline controls the 640 px window sweep found outside the Settings and crew
 * stylesheets were 18 to 20 px tall. They get the same 24 px inline box the crew links have. jsdom applies no stylesheet,
 * so each rule is read from its source, the way designTargetSize.test.ts and settingsTargetSize.test.ts read theirs.
 */
const WCAG_MINIMUM_PX = 24;

const read = (path: string) => readFileSync(join(process.cwd(), "src", path), "utf8");

const tokens = read("styles/tokens.css");

/** A px token from tokens.css. */
function tokenPx(name: string): number {
  const found = new RegExp(`--${name}:\\s*(\\d+(?:\\.\\d+)?)px`).exec(tokens)?.[1];
  if (found === undefined) throw new Error(`--${name} is not a pixel token`);
  return Number(found);
}

/** The declaration bodies of every rule whose selector list names `selector` exactly, comments removed. */
function bodies(css: string, selector: string): string[] {
  return Array.from(css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g))
    .filter(([, group = ""]) => group.split(",").some((part) => part.replace(/\s+/g, " ").trim() === selector))
    .map(([, , body = ""]) => body);
}

/** The min-height a body asks for in px, a literal or through the tokens.css value behind `var(--token)`. */
function minimumPx(body: string, where: string): number {
  const value = /(?:^|;)\s*min-height:\s*([^;]+)/.exec(body)?.[1]?.trim();
  expect(value, `${where} needs min-height`).toBeDefined();
  const literal = /^(\d+(?:\.\d+)?)px$/.exec(value ?? "")?.[1];
  if (literal !== undefined) return Number(literal);
  const token = /^var\(--([\w-]+)\)$/.exec(value ?? "")?.[1];
  if (token !== undefined) return tokenPx(token);
  throw new Error(`${where}'s min-height ${String(value)} is not a size this test can read`);
}

/**
 * The inline targets the sweep found, with the file whose rule sizes each. The Tasks page draws the Settings section's
 * table outside .settings-page, so its rule takes the same hit token instead of the inline box the text links use.
 */
const INLINE_TARGETS = [
  { path: "styles/model-picker.css", selector: ".hero-link", inline: true, what: "the home heading's way out of a project" },
  { path: "styles/composer.css", selector: ".context-action", inline: true, what: "the composer's checkout, repositories and permission links" },
  { path: "features/settings/ScheduledTasks.css", selector: ".tasks-page .set-link", inline: false, what: "a task's name on the Tasks page" },
  { path: "features/usage/usage.css", selector: ".usage-table .text-action", inline: true, what: "the usage table's Set price link" },
  { path: "styles/thread-overlays.css", selector: ".diff-count", inline: true, what: "the changed-files toggle in the transcript" },
] as const;

describe("the inline controls' target size", () => {
  it.each(INLINE_TARGETS)("sizes $what at or above the WCAG minimum", ({ path, selector, what }) => {
    const rules = bodies(read(path), selector).filter((body) => /(?:^|;)\s*min-height:/.test(body));
    expect(rules, `${path}: ${selector} (the ${what}) needs a min-height`).not.toHaveLength(0);
    for (const body of rules) expect(minimumPx(body, `${path}: ${selector}`)).toBeGreaterThanOrEqual(WCAG_MINIMUM_PX);
  });

  it.each(INLINE_TARGETS.filter((one) => one.inline))(
    "keeps $what inline and centres its label in the taller box",
    ({ path, selector }) => {
      const sized = bodies(read(path), selector).filter((body) => body.includes("min-height"));
      expect(sized).not.toHaveLength(0);
      const body = sized.join(";");
      expect(body, `${path}: ${selector} needs display: inline-flex`).toMatch(/(?:^|;)\s*display:\s*inline-flex/);
      expect(body, `${path}: ${selector} needs align-items: center`).toMatch(/(?:^|;)\s*align-items:\s*center/);
    },
  );
});
