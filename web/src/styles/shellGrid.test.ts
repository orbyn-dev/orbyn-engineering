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

describe("what sits under the floating title bar", () => {
  it("starts the settings search 12px below the 44px bar", () => {
    const side = rules.find(({ selector }) => selector === ".settings-side");
    expect(side?.body).toMatch(/padding:\s*calc\(var\(--titlebar-h\) \+ 12px\) 10px 12px/);
    expect(readFileSync(join(process.cwd(), "src/styles/tokens.css"), "utf8")).toContain("--titlebar-h: 44px;");
  });
});

describe("the Windows bar", () => {
  it("is 32px, with the settings search and page headers following it", () => {
    const css = readFileSync(join(process.cwd(), "src/styles/windowFrame.css"), "utf8");
    expect(css).toMatch(/:root\[data-window-frame="windows"\]\s*\{[^}]*--titlebar-h:\s*32px;[^}]*--topbar-h:\s*32px;/);
    expect(css).toMatch(/\.window-control:focus-visible[^}]*outline-offset:\s*-2px/);
  });

  it("draws the Maximize states the shell reports while its hit-test window holds the pointer", () => {
    // Windows offers the snap layouts only to the window that answers the hit test, and the shell's
    // window over the button holds the pointer, so the page never sees :hover or :active there. The
    // shell reports the state on the root element (desktop/src-tauri/src/snap_overlay.rs) and these
    // rules draw the design's two states unchanged.
    const body = (selector: string) => rules.find((rule) => rule.selector === selector)?.body;
    const hover = body(".window-control:hover");
    const press = body(".window-control:active");
    expect(hover).toBeTruthy();
    expect(press).toBeTruthy();
    expect(body('[data-window-control="hover"] .window-control-maximize')).toBe(hover);
    expect(body('[data-window-control="press"] .window-control-maximize')).toBe(press);
  });
});

describe("the pull request list's optional columns", () => {
  it("empties a hidden column's track so the grid keeps no gap for it", () => {
    const table = rules.filter(({ selector }) => selector === ".pulls-table[data-no-reviewers]" || selector === ".pulls-table[data-no-review]");
    expect(table.map(({ body }) => body.trim().replace(/\s+/g, " "))).toEqual(["--c-rev: ; --c-rev-n: ;", "--c-yours: ; --c-yours-n: ;"]);
    const templates = rules.flatMap(({ body }) => [...body.matchAll(/--pulls-cols:\s*([^;]+);/g)].map((match) => match[1] ?? ""));
    // Every table template that has the two tracks takes them from the variables, wherever the breakpoint.
    for (const template of templates.filter((one) => /132px|28px/.test(one))) expect(template, template).toMatch(/var\(--c-yours\)/);
    // The icon steps take the narrow tracks, emptied the same way.
    for (const template of templates.filter((one) => /36px/.test(one))) expect(template, template).toMatch(/var\(--c-yours-n\)/);
  });
});