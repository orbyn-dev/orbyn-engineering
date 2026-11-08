import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const stripped = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");
const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)]));
const rules = walk(join(process.cwd(), "src"))
  .filter((file) => file.endsWith(".css"))
  .flatMap((file) => [...stripped(readFileSync(file, "utf8")).matchAll(/([^{}]+)\{([^}]*)\}/g)].map((match) => ({ file, selector: (match[1] ?? "").trim().replace(/\s+/g, " "), body: match[2] ?? "" })));

describe("where the main area sits in the shell grid", () => {
  it("never puts .main in a second row except on the phone, where the title bar has a row of its own", () => {
    const second = rules.filter(({ selector, body }) => /(^|[ >])\.main$/.test(selector) && /grid-area:\s*2 \/ 1/.test(body));
    // The unified bar floats over the page in every other layout, so the grid has one row: a .main in row 2 sinks
    // below an empty row and the page renders blank (the reading layout did).
    for (const rule of second) expect(rule.selector, `${rule.file}: ${rule.selector}`).toMatch(/data-layout="phone"/);
  });

  it("places the reading layout's folded-sidebar main area in the first row", () => {
    const reading = rules.find(({ selector }) => selector.startsWith(':root[data-workspace="reading"] .shell:not([data-reading-nav="open"])') && selector.endsWith("> .main"));
    expect(reading?.body).toMatch(/grid-area:\s*1 \/ 1/);
  });
});
