import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * One page-title scale and one section-heading style (#1115). docs/design.md draws one page heading, a 22px medium
 * title with -0.015em tracking, and gives the open Settings section that same 22px title; every size stays on the
 * scale in tokens.css. Settings draws its visible title in a span with the page's own h1 left to the screen reader,
 * so a feature stylesheet can size that title apart from the one every other page draws, and the checks below fail
 * when one does. jsdom applies no stylesheet, so the rules are read from their source, the way the target-size tests
 * read theirs.
 */

const SRC = join(process.cwd(), "src");

function walk(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)],
  );
}

/** Every stylesheet under src, by its path from src. */
const STYLESHEETS = Object.fromEntries(
  walk(SRC)
    .filter((file) => file.endsWith(".css"))
    .map((file) => [relative(SRC, file).replaceAll("\\", "/"), readFileSync(file, "utf8")]),
);

interface Rule {
  /** The stylesheet, by its path from src. */
  file: string;
  /** One selector from the rule's selector list, trimmed. */
  selector: string;
  /** The rule's declarations, comments removed. */
  body: string;
}

/** Every selector of every rule under src, in source order. */
const RULES: Rule[] = Object.entries(STYLESHEETS).flatMap(([file, css]) =>
  Array.from(css.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g)).flatMap(([, list = "", body = ""]) =>
    list.split(",").map((selector) => ({ file, selector: selector.trim(), body })),
  ),
);

const tokens = STYLESHEETS["styles/tokens.css"] ?? "";

/** A px token from tokens.css. */
function tokenPx(name: string): number {
  const found = new RegExp("--" + name + ":\\s*(\\d+(?:\\.\\d+)?)px").exec(tokens)?.[1];
  if (found === undefined) throw new Error("--" + name + " is not a pixel token");
  return Number(found);
}

/** The one rule that names a selector exactly, from the stylesheet that owns it. */
function rule(file: string, selector: string): Rule {
  const found = RULES.filter((one) => one.file === file && one.selector === selector);
  expect(found, file + ": " + selector).toHaveLength(1);
  return found[0] as Rule;
}

/** A declaration's value from a rule body, whitespace collapsed, or undefined when the rule sets none. */
function declaration(body: string, property: string): string | undefined {
  return new RegExp("(?:^|;)\\s*" + property + ":\\s*([^;]+)").exec(body)?.[1]?.trim().replace(/\s+/g, " ");
}

/** A letter-spacing value in em, however the leading zero is written (-.015em or -0.015em). */
function em(value: string | undefined): number {
  return Number.parseFloat((value ?? "").replace(/em$/, ""));
}

/** The rules that draw a page title: the shared page heading the crumb marks (the thread topbar and PageBar), a skill page's own heading, and the Settings title. */
const PAGE_TITLES = [
  ["styles/thread.css", ".crumbs h1"],
  ["styles/thread.css", ".crumb-title"],
  ["styles/pages.css", ".settings-title"],
  ["styles/pages.css", ".settings-main > h1"],
  ["features/skills/skills.css", ".skills-heading h1"],
] as const;

describe("the page-title scale", () => {
  it("is the design's 22px medium title from the token scale", () => {
    expect(tokenPx("text-page")).toBe(22);
  });

  it.each(PAGE_TITLES)("%s draws %s on the one page-title rule", (file, selector) => {
    const body = rule(file, selector).body;
    expect(declaration(body, "font-size"), "font-size").toBe("var(--text-page)");
    expect(declaration(body, "font-weight"), "font-weight").toBe("500");
    expect(em(declaration(body, "letter-spacing")), "letter-spacing").toBeCloseTo(-0.015, 5);
  });

  it("leaves both Settings titles to the shared rule, so no feature styles them apart", () => {
    // Settings draws its visible title in the .settings-title span because its h1 stays for the screen reader; a
    // rule that sized either of them again drew Settings off the shared scale (#1115). Both take the title rule.
    // A feature stylesheet can reach a title through a qualified selector too, and .settings-page .settings-title
    // outranks the shared rule, so a selector counts whether it names the hook alone or qualifies it.
    /** A selector with one space around each combinator, so `a>b` and `a > b` compare the same. */
    const normalized = (selector: string) => selector.replace(/\s*([>+~])\s*/g, " $1 ").replace(/\s+/g, " ").trim();
    const hooks = [".settings-title", ".settings-main > h1"].map(normalized);
    const sizing = RULES.filter(
      (one) => hooks.some((hook) => normalized(one.selector).endsWith(hook)) && declaration(one.body, "font-size") !== undefined,
    );
    expect(sizing.map((one) => one.file + ": " + one.selector)).toEqual([
      "styles/pages.css: .settings-title",
      "styles/pages.css: .settings-main > h1",
    ]);
  });
});

/**
 * The section headings a page draws over its own content (Usage's plot, summary and breakdown sections, a skill's
 * sections, and the shared settings group heading) are one style: 13px at weight 500 in ink. They are not the quieter
 * group labels below, which label a list the way the settings and Limits frames draw them.
 */
const SECTION_HEADINGS = [
  ["styles/pages.css", ".setting-group > h2"],
  ["styles/pages.css", ".settings-body .section > h2"],
  ["features/usage/usage.css", ".usage-plot h2"],
  ["features/usage/usage.css", ".usage-summary h2"],
  ["features/usage/usage.css", ".usage-section h2"],
  ["features/skills/skills.css", ".skills-detail h2"],
] as const;

describe("the section-heading style", () => {
  it("uses the 13px step at medium weight, never the muted label step", () => {
    expect(tokenPx("text-small")).toBe(13);
    for (const [file, selector] of SECTION_HEADINGS) {
      const body = rule(file, selector).body;
      expect(declaration(body, "font-size"), file + ": " + selector + " font-size").toBe("var(--text-small)");
      expect(declaration(body, "font-weight"), file + ": " + selector + " font-weight").toBe("500");
      expect(body, file + ": " + selector + " must not read as a quieter label").not.toMatch(/--color-(?:muted|faint)/);
    }
  });
});

describe("the Settings group label", () => {
  it("keeps the quiet label the settings frames draw instead of a section heading", () => {
    // Engineer Designs v27 draws a titled settings group as a quiet label between lists (the s-labs and s-x-keys-*
    // frames among others, which is why the rule sits in settingsDesign.css and not pages.css). It labels the group
    // rather than heading a section, so it stays 11.5px, weight 400 and muted as drawn; changing it would be a design
    // change, not this issue's one section-heading rule (#1115 reports the difference without a design ruling).
    const body = rule("features/settings/settingsDesign.css", ".settings-page .setting-group > h2").body;
    expect(declaration(body, "font-size")).toBe("var(--text-label)");
    expect(declaration(body, "font-weight")).toBe("400");
    expect(declaration(body, "color")).toBe("var(--color-muted)");
  });
});
