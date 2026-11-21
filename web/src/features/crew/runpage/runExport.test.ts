import { describe, expect, it, vi, afterEach } from "vitest";
import { HOSTILE, taskDetail } from "./fixture";
import { RUN_COMMENT_CAP, cleanText, fileNameOf, runPageHtml, runPageMarkdown, runPageModel } from "./runExport";

const NOW = "2026-11-07T15:03:00Z";
afterEach(() => vi.restoreAllMocks());

describe("cleanText", () => {
  it("masks key-shaped words and key=value pairs", () => {
    const out = cleanText("use ghp_abcdefghijklmnopqrstuvwxyz0123456789 and token=hunter2value or Bearer abcdefghijklmnop1234 ok", 500);
    expect(out).not.toMatch(/ghp_|hunter2|abcdefghijklmnop1234/);
    expect(out).toContain("use");
    expect(out).toContain("ok");
  });
  it("keeps a repo-relative path and hides a machine path", () => {
    expect(cleanText("src/a.ts", 100)).toBe("src/a.ts");
    for (const bad of ["/Users/anhdang/repo/a.ts", "C:\\Users\\bob\\a.ts", "see /home/bob/work/a.ts now", "~/work/a.ts"]) {
      const out = cleanText(bad, 100);
      expect(out).not.toMatch(/anhdang|bob|Users|home\//);
    }
  });
  it("caps the length, drops control and bidi characters and folds newlines", () => {
    const out = cleanText(`a\u202Eb\u0000c\nd\r\ne${"x".repeat(900)}`, 40);
    expect(out.length).toBeLessThanOrEqual(40);
    for (const bad of ["\u202E", "\u0000", "\n", "\r"]) expect(out.includes(bad)).toBe(false);
    expect(out.endsWith("…")).toBe(true);
  });
});

describe("fileNameOf", () => {
  it("is a safe name from sanitized text", () => {
    expect(fileNameOf("Use next in the auth redirect")).toBe("use-next-in-the-auth-redirect-run.html");
    expect(fileNameOf("../../etc/passwd\u0000")).toBe("etc-passwd-run.html");
    expect(fileNameOf("???")).toBe("crew-run.html");
    expect(fileNameOf("a".repeat(300)).length).toBeLessThanOrEqual(60);
    expect(fileNameOf("CON:*?<>|\"")).not.toMatch(/[\\/:*?"<>|]/);
  });
});

describe("the static page", () => {
  const html = runPageHtml(runPageModel(taskDetail({ row: { ...taskDetail().row, name: HOSTILE }, brief: { purpose: `${HOSTILE} key=sk-abcdefghijklmnopqrstuvwx1234`, files: ["/Users/anhdang/x"], budget_turns: 1, budget_minutes: 1 } }), NOW));
  it("has no script, no remote resource and no active element", () => {
    expect(html.startsWith("<!doctype html>")).toBe(true);
    const tags = [...html.matchAll(/<\/?([a-z0-9]+)([^>]*)>/gi)];
    const allowed = new Set(["html", "head", "meta", "title", "style", "body", "main", "h1", "h2", "p", "span", "dl", "dt", "dd", "ul", "li", "strong", "code", "br"]);
    for (const [, name, attrs] of tags) {
      expect(allowed.has(name!.toLowerCase())).toBe(true);
      expect(attrs).not.toMatch(/\son\w+=|javascript:|src=|href=|url\(/i);
    }
    expect(html).not.toMatch(/<[^>]*\sstyle=/i);
    expect(html).not.toMatch(/@import|<\/?script/i);
    expect(html).toContain("default-src 'none'");
    expect(html).not.toMatch(/https?:\/\/[^\s<]*\.(js|css|png)/);
  });
  it("escapes the hostile text as text", () => {
    expect(html).toContain("&lt;img src=x onerror=alert(1)&gt;");
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toMatch(/sk-abcdefghij|anhdang/);
  });
  it("says what is Planned and makes up no value for it", () => {
    expect(html).toContain("Planned");
    expect(html).toMatch(/not verified|Not verified/);
    expect(html).not.toMatch(/7F3A|orbyn\.dev\/crew-result/);
    expect(html).toContain("snapshot");
  });
  it("shows the counts, route, findings and the state", () => {
    const plain = runPageHtml(runPageModel(taskDetail(), NOW));
    for (const word of ["3 of 3", "Sonnet 5.5", "next accepts //host as a path", "auth/redirect.ts:18", "Passed"]) expect(plain).toContain(word);
  });
  it("says a failed run failed first", () => {
    const failed = runPageHtml(runPageModel(taskDetail({ row: { ...taskDetail().row, state: "failed", cells: [{ stage: "checks", state: "failed", passed: 2, of: 3 }] } }), NOW));
    expect(failed.indexOf("Failed")).toBeGreaterThan(-1);
    expect(failed.indexOf("Failed")).toBeLessThan(failed.indexOf("Findings"));
  });
});

describe("the PR comment", () => {
  const model = runPageModel(taskDetail({ row: { ...taskDetail().row, name: HOSTILE }, brief: { purpose: `${HOSTILE} password=swordfish99`, files: [], budget_turns: 1, budget_minutes: 1 } }), NOW);
  const md = runPageMarkdown(model, 142);
  it("neutralises markdown, html, links, mentions and references", () => {
    expect(md).not.toMatch(/(^|[^\\])</);
    expect(md).not.toContain("](");
    expect(md).not.toMatch(/@octocat/);
    expect(md).not.toMatch(/https?:\/\//);
    expect(md).not.toMatch(/www\./);
    expect(md).not.toMatch(/#123/);
    expect(md).not.toContain("swordfish99");
  });
  it("names the pull request without a local path and says nothing was posted", () => {
    expect(md).toContain("#142");
    expect(md).not.toMatch(/\/Users\/|file:/);
  });
  it("stays under the cap however many findings there are", () => {
    const many = Array.from({ length: 200 }, (_, n) => ({ id: `f${n}`, lane: "code", round: 1, severity: "low", path: `src/file${n}.ts`, line: n, claim: "x".repeat(300), check: "confirmed", status: "open", blocks: true }));
    const big = runPageMarkdown(runPageModel(taskDetail({ findings: many }), NOW), null);
    expect(big.length).toBeLessThanOrEqual(RUN_COMMENT_CAP);
    expect(big).toMatch(/more/);
  });
});
