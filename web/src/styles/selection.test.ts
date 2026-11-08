import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

const css = readFileSync(join(process.cwd(), "src/styles/selection.css"), "utf8").replace(/\/\*[\s\S]*?\*\//g, "");
const blocks = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)].map((match) => ({
  selectors: (match[1] ?? "").split(",").map((entry) => entry.trim()),
  body: match[2] ?? "",
}));
const optedIn = new Set(blocks.filter(({ body }) => /(^|\s)user-select:\s*text/.test(body)).flatMap(({ selectors }) => selectors));

describe("what can be selected", () => {
  it("turns selection and the text cursor off at the app root, with the WebKit spelling", () => {
    const root = blocks.find(({ selectors }) => selectors.includes("#root"));
    expect(root?.selectors).toEqual(expect.arrayContaining(["html", "body", "#root"]));
    expect(root?.body).toMatch(/-webkit-user-select:\s*none/);
    expect(root?.body).toMatch(/(^|\s)user-select:\s*none/);
    expect(root?.body).toMatch(/cursor:\s*default/);
  });

  it("opts the text people copy back in, WebKit spelling included", () => {
    for (const selector of [
      ".msg-bubble", ".md", ".code-card", ".tool-output", ".tool-inspect", ".command", ".diff-view", ".diff-card",
      ".file-preview", ".pull-body", ".pull-comment", ".pull-review-note-text", ".pull-check-name", ".pull-patch",
      ".error-text", ".error-state", ".set-path", ".terminal-host", "input", "textarea", "[contenteditable]",
    ]) expect(optedIn, selector).toContain(selector);
    expect(css).toMatch(/-webkit-user-select:\s*text/);
  });

  it("leaves link and button cursors alone", () => {
    expect(css).not.toMatch(/cursor:\s*pointer/);
  });
});
