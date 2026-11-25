import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Target size (WCAG 2.2 AA 2.5.8): the crew screens' inline links and buttons are at least 24 x 24 px, so a click lands
 * without changing the type size or taking the control out of its line. jsdom applies no stylesheet, so the shared design
 * rule is read from its source, the way the Settings target-size test reads its own.
 */
const WCAG_MINIMUM_PX = 24;

const design = readFileSync(join(process.cwd(), "src", "features", "crew", "shared", "design.css"), "utf8");

/** The declaration bodies of every rule whose selector list names `selector` exactly, comments removed. */
function bodies(css: string, selector: string): string[] {
  return Array.from(css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g))
    .filter(([, group = ""]) => group.split(",").some((part) => part.trim() === selector))
    .map(([, , body = ""]) => body);
}

describe("the crew inline links' target size", () => {
  it("gives the shared design class a 24 px box and keeps it inline", () => {
    const rules = bodies(design, ".cx-t10r");
    expect(rules.length).toBeGreaterThan(0);
    const body = rules.join(";");
    // Inline, so the link stays in the heading or the sentence it belongs to, with its label centred in the taller box.
    expect(body).toMatch(/(?:^|;)\s*display:\s*inline-flex/);
    expect(body).toMatch(/(?:^|;)\s*align-items:\s*center/);
    const minHeight = /(?:^|;)\s*min-height:\s*(\d+(?:\.\d+)?)px/.exec(body)?.[1];
    expect(Number(minHeight), "min-height must clear the WCAG minimum").toBeGreaterThanOrEqual(WCAG_MINIMUM_PX);
    // The audit is about the hit area only: the design's type size stays as drawn.
    expect(body).not.toMatch(/(?:^|;)\s*font-size:/);
  });
});
