import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * jsdom has no layout and Vitest does not load stylesheets, so the rules the on-screen check needs are read here; the
 * browser case that measures them is web/e2e/settings-anchor.spec.ts. The ring and halo are CSS only, so a rule that loses
 * the cascade or a scroller without room for the ring is invisible to every component test.
 */
const read = (path: string) => readFileSync(join(process.cwd(), "src", path), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const settings = read("features/settings/settingsDesign.css");
const pages = read("styles/pages.css");

/** The declaration body of the first rule whose selector list names `selector` exactly. */
const rule = (css: string, selector: string) => {
  for (const [, selectors = "", body = ""] of css.matchAll(/([^{}]+)\{([^{}]*)\}/g)) {
    if (selectors.split(",").some((part) => part.trim() === selector)) return body;
  }
  return "";
};

/** The body of the first `@media (max-width: <width>px)` block, with nested braces matched. */
function media(css: string, width: number): string {
  const open = new RegExp(`@media\\s*\\(max-width:\\s*${width}px\\)\\s*\\{`).exec(css);
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

/** A property's px value from a declaration body. */
function px(body: string, property: string): number {
  const found = new RegExp(`(?:^|;)\\s*${property}:\\s*(-?\\d+(?:\\.\\d+)?)px`).exec(body)?.[1];
  if (found === undefined) throw new Error(`the rule has no ${property} in px`);
  return Number(found);
}

describe("the row an address names (pb-k55-row-anchor)", () => {
  it("draws the designed ring in the pointer blue, inset, with the halo as a half-second animation", () => {
    const anchored = rule(settings, '.settings-page [data-anchored="true"]');
    expect(anchored).toMatch(/box-shadow:\s*inset 0 0 0 2px var\(--color-blueSolid\)/);
    expect(anchored).toMatch(/background:\s*var\(--color-selected\)/);
    expect(anchored).toMatch(/animation:\s*set-anchor-halo 500ms ease-out/);
    expect(settings).toMatch(/@keyframes set-anchor-halo\s*\{[\s\S]*?0 0 0 5px color-mix\(in srgb, var\(--color-blueSolid\) 30%, transparent\)/);
  });

  it("suppresses the native [tabindex] focus ring that ties the anchored row's specificity", () => {
    // The anchored row declares outline: none, and base.css draws a 2px ring for [tabindex]:focus-visible with the same
    // specificity; that ring comes later in the cascade, so it applied anyway. The :focus-visible rule wins the tie and
    // keeps the designed inset ring as the only indicator.
    expect(rule(settings, '.settings-page [data-anchored="true"]')).toMatch(/outline:\s*none/);
    expect(rule(settings, '.settings-page [data-anchored="true"]:focus-visible')).toMatch(/outline:\s*none/);
  });

  it("draws a real focus outline in forced colors, where the inset shadow is dropped", () => {
    const forced = /@media\s*\(forced-colors:\s*active\)\s*\{([^}]*)\}/.exec(settings)?.[1] ?? "";
    expect(forced).toMatch(/\.settings-page \[data-anchored="true"\]:focus-visible \{\s*outline: 2px solid CanvasText;\s*outline-offset: -2px;/);
  });

  it("leaves the halo out with reduced motion and keeps the ring", () => {
    const reduced = /@media\s*\(prefers-reduced-motion:\s*reduce\)\s*\{([^}]*)\}/.exec(settings)?.[1] ?? "";
    expect(reduced).toMatch(/\.settings-page \[data-anchored="true"\] \{\s*animation: none;/);
  });
});

describe("the phone settings section row (pb-k55-chips)", () => {
  it("gives a tabbed chip's ring room inside the row's scroller instead of clipping it", () => {
    const phone = media(pages, 599);
    const nav = (() => {
      for (const [, selectors = "", body = ""] of phone.matchAll(/([^{}]+)\{([^{}]*)\}/g)) if (selectors.trim() === ".settings-nav") return body;
      return "";
    })();
    // The scroller clips its content, so the room must be inside the clip: the 2px ring and its 2px offset need 4px.
    expect(nav).toMatch(/overflow-y:\s*hidden/);
    expect(px(nav, "padding")).toBeGreaterThanOrEqual(4);
    expect(px(nav, "margin")).toBeLessThanOrEqual(-4);
    expect(px(nav, "scroll-padding")).toBeGreaterThanOrEqual(4);
  });
});
