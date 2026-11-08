import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { expect, it } from "vitest";

const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)]));
const stripped = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");
const sheets = walk(join(process.cwd(), "src")).filter((file) => file.endsWith(".css"));
const base = stripped(readFileSync(join(process.cwd(), "src/styles/base.css"), "utf8"));
const rule = /:is\(([^)]*)\)\s+input:not\(\[type="checkbox"\], \[type="radio"\]\)\s*\{([^}]*)\}/.exec(base);
const shared = rule?.[1] ?? "";
const sharedBody = rule?.[2] ?? "";

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
  const exempt = new Set([".project-image-picker", ".question"]);
  expect(wrappers.filter((wrapper) => !exempt.has(wrapper) && !shared.includes(wrapper))).toEqual([]);
});

it("resets only what the global rule leaks, so a pill keeps its own type and height", () => {
  expect(rule).not.toBeNull();
  for (const property of ["border: 0", "background: transparent", "padding: 0", "min-height: 0"]) expect(sharedBody).toContain(property);
  expect(sharedBody).not.toMatch(/font-size|(^|\s)height:|width/);
});
