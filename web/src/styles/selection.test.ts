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
      ".error-text", ".set-path", ".terminal-host", "input", "textarea", ".approval-target", ".approval-kv",
      ".ask-label", ".skills-description", ".skills-detail", '[contenteditable]:not([contenteditable="false"])',
    ]) expect(optedIn, selector).toContain(selector);
    expect(optedIn).not.toContain(".error-state");
    expect(css).toMatch(/-webkit-user-select:\s*text/);
  });

  it("shows the text cursor over fields and the text blocks people copy, not the root's arrow", () => {
    const texty = new Set(blocks.filter(({ body }) => /(^|\s)cursor:\s*text/.test(body)).flatMap(({ selectors }) => selectors));
    for (const selector of ["textarea", ".command", ".diff-card", ".pull-patch", ".approval-target", ".set-path", ".msg-bubble"]) {
      expect(texty, selector).toContain(selector);
    }
    expect([...texty].some((selector) => selector.startsWith("input:not("))).toBe(true);
    expect(texty).toContain('[contenteditable]:not([contenteditable="false"])');
  });

  it("leaves link and button cursors alone", () => {
    expect(css).not.toMatch(/cursor:\s*pointer/);
  });
});
