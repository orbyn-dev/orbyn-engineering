import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const stripped = (text: string) => text.replace(/\/\*[\s\S]*?\*\//g, "");
const walk = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)]));
const rules = walk(join(process.cwd(), "src"))
  .filter((file) => file.endsWith(".css"))
  .flatMap((file) => [...stripped(readFileSync(file, "utf8")).matchAll(/([^{}]+)\{([^}]*)\}/g)].map((match) => ({ file, selector: (match[1] ?? "").trim().replace(/\s+/g, " "), body: match[2] ?? "" })));

describe("the toast layer", () => {
  it("lays clone jobs out inside the toast stack, never as a fixed layer of their own", () => {
    const groups = rules.filter(({ selector }) => selector === ".clone-jobs");
    expect(groups).toHaveLength(1);
    // A fixed offset of its own is what put the clone toast over the sidebar's Engines chip.
    const body = groups[0]!.body;
    expect(body, groups[0]!.file).not.toMatch(/position:\s*(fixed|absolute|sticky)/);
    expect(body, groups[0]!.file).not.toMatch(/(^|[;{\s])(left|right|top|bottom|inset|z-index)\s*:/);
  });

  it("anchors the one stack clear of the sidebar on a page that shows it", () => {
    const stack = rules.find(({ selector }) => selector === ".toasts");
    expect(stack?.body).toMatch(/position:\s*fixed/);
    // The stack starts at the sidebar's right edge, so the Engines chip in the sidebar footer is
    // not under it; below 900px the sidebar is a drawer and the stack goes to the window's edge.
    const besideSidebar = rules.find(({ selector, body }) =>
      selector.includes('.shell[data-page="home"] .toasts') && /left:\s*calc\(var\(--sidebar-w\) \+ 16px\)/.test(body));
    expect(besideSidebar, "the stack starts at the sidebar's right edge").toBeDefined();
  });
});
