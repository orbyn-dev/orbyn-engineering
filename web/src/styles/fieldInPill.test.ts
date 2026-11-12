import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)]));
const stripped = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");
const sheets = walk(join(process.cwd(), "src")).filter((file) => file.endsWith(".css"));
const base = stripped(readFileSync(join(process.cwd(), "src/styles/base.css"), "utf8"));
const rule = /:is\(([^)]*)\)\s+input:not\(\[type="checkbox"\], \[type="radio"\]\):not\(\[type="hidden"\]\),\s*([^{]*)\{([^}]*)\}/.exec(base);
const shared = `${rule?.[1] ?? ""} ${rule?.[2] ?? ""}`;
const sharedBody = rule?.[3] ?? "";

/** Wrappers whose input is itself the visible field (it keeps its own fill, radius and padding), or is no pill. */
const EXEMPT: Record<string, string> = {
  ".thread-find": "its input is the filled field beside the match count",
  ".browser-bar": "the address field is the filled input itself",
  ".filter-popup": "the popup's input sits on the popup's own surface",
  ".question": "an answer field, not a search pill",
  ".project-image-picker": "a direct child field of the picker",
};

/** The wrapper of every `.wrapper input { border: 0 }` rule: the pills that draw their own field. */
function bareInputWrappers(): string[] {
  const found = new Set<string>();
  for (const file of sheets) {
    for (const match of stripped(readFileSync(file, "utf8")).matchAll(/([^{}]+)\{([^}]*)\}/g)) {
      const selector = (match[1] ?? "").trim().replace(/\s+/g, " ");
      if (!/\binput$/.test(selector) || /:|\[/.test(selector.replace(/\binput$/, "")) || !/border:\s*(0|none)\b/.test(match[2] ?? "")) continue;
      const wrapper = selector.replace(/ ?>? ?input$/, "").split(" ").pop() ?? "";
      if (/^\.[\w-]+$/.test(wrapper)) found.add(wrapper);
    }
  }
  return [...found];
}

it("keeps the global input fill, border and padding out of every search or filter pill", () => {
  const wrappers = bareInputWrappers();
  expect(shared).toContain(".set-find");
  expect(wrappers.length).toBeGreaterThan(8);
  expect(wrappers.filter((wrapper) => !(wrapper in EXEMPT) && !shared.includes(wrapper))).toEqual([]);
  // An exemption must still be a wrapper with a bare input, and must not also be in the shared rule.
  for (const wrapper of Object.keys(EXEMPT)) expect(shared, wrapper).not.toContain(wrapper);
});

it("resets only what the global rule leaks, so a pill keeps its own type and height", () => {
  expect(rule).not.toBeNull();
  for (const property of ["border: 0", "background: transparent", "padding: 0", "min-height: 0"]) expect(sharedBody).toContain(property);
  expect(sharedBody).not.toMatch(/font-size|(^|\s)height:|width/);
});
