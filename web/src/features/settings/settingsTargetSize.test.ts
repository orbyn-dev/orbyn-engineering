import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Target size (WCAG 2.2 AA 2.5.8): every interactive target is at least 24 x 24 px. Settings takes its sizes from the
 * design's hit token (--hit), never from a literal smaller than the minimum, so a click lands without widening the type.
 */
const WCAG_MINIMUM_PX = 24;

const read = (path: string) => readFileSync(join(process.cwd(), "src", path), "utf8");

const tokens = read("styles/tokens.css");

/** The stylesheets that size a Settings control, including the shared ones it borrows rules from. */
const SETTINGS_CSS = [
  "features/settings/settingsDesign.css",
  "features/settings/ProviderInstances.css",
  "features/settings/ScheduledTasks.css",
  "features/settings/browser/browserProfiles.css",
  "features/settings/secrets/Secrets.css",
  "features/settings/ProjectIconPicker.css",
  "features/settings/ProjectImagePicker.css",
  "features/settings/engineHost/engineHost.css",
  "styles/pages.css",
  "styles/tools.css",
];

/** The small text and icon controls of a Settings page, by the class each one carries. */
const SMALL_CONTROLS = [
  ".set-link",
  ".set-red-link",
  ".set-row-open",
  ".tg-toggle",
  ".tg-open-skills",
  ".settings-crumbs a",
  ".setting-inheritance-actions .icon-btn",
];

/** A px token from tokens.css. */
function tokenPx(name: string): number {
  const found = new RegExp(`--${name}:\\s*(\\d+(?:\\.\\d+)?)px`).exec(tokens)?.[1];
  if (found === undefined) throw new Error(`--${name} is not a pixel token`);
  return Number(found);
}

/** The declaration bodies of every rule whose selector list names `selector`, comments removed. */
function bodies(text: string, selector: string): string[] {
  return Array.from(text.replace(/\/\*[\s\S]*?\*\//g, "").matchAll(/([^{}]+)\{([^{}]*)\}/g))
    .filter(([, group = ""]) => group.split(",").some((part) => part.includes(selector)))
    .map(([, , body = ""]) => body);
}

/** The size a rule asks for, when it is a literal px value. */
function literalSizes(body: string, property: string): number[] {
  return Array.from(body.matchAll(new RegExp(`(?:^|;)\\s*${property}:\\s*(\\d+(?:\\.\\d+)?)px`, "g")), ([, value]) => Number(value));
}

describe("interactive target size in Settings", () => {
  it("keeps the designed hit target at or above the WCAG minimum", () => {
    expect(tokenPx("hit")).toBeGreaterThanOrEqual(WCAG_MINIMUM_PX);
  });

  it("gives every small Settings control the token minimum", () => {
    const settings = read("features/settings/settingsDesign.css");
    for (const selector of SMALL_CONTROLS.filter((one) => !one.includes("icon-btn"))) {
      const sized = bodies(settings, selector).filter((body) => body.includes("min-height: var(--hit)") && body.includes("min-width: var(--hit)"));
      expect(sized, `${selector} needs min-height and min-width from var(--hit)`).not.toHaveLength(0);
    }
  });

  it.each(SMALL_CONTROLS)("sizes %s with the token, never with a smaller literal", (selector) => {
    for (const path of SETTINGS_CSS) {
      for (const body of bodies(read(path), selector)) {
        for (const property of ["height", "min-height", "width", "min-width"]) {
          for (const size of literalSizes(body, property)) {
            expect(size, `${path}: ${selector} sets ${property}: ${size}px`).toBeGreaterThanOrEqual(WCAG_MINIMUM_PX);
          }
        }
      }
    }
  });
});
