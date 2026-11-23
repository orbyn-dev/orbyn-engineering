import { describe, expect, it, vi, afterEach } from "vitest";
import { HOSTILE, runPage, runningRunPage, taskDetail } from "./fixture";
import { RUN_COMMENT_CAP, WITHHELD, cleanText, fileNameOf, runPageHtml, runPageMarkdown, runPageModel } from "./runExport";

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

describe("the model with a run read", () => {
  const model = runPageModel(taskDetail(), NOW, runPage());
  it("fills the prompt, checks, not-verified lines, commit and pull request instead of Planned", () => {
    expect(model.hasRun).toBe(true);
    expect(model.prompt?.text).toBe("Return people to the page they came from. Add tests.");
    expect(model.checkList?.map((one) => one.command)).toEqual(["pnpm lint", "pnpm test auth"]);
    expect(model.checkList?.[0]).toMatchObject({ outcome: "pass", exitCode: 0, duration: "4 s", proves: "The changed files follow the style rules.", commit: "a41c9e2" });
    expect(model.checkList?.[1]).toMatchObject({ outcome: "fail", exitCode: 1, duration: "22 s", proves: "next=//evil.com is rejected." });
    expect(model.notVerified).toEqual(["The mobile app’s redirect: no check covers it."]);
    expect(model.commit).toBe("a41c9e2");
    expect(model.pull).toEqual({ number: 142, branch: "crew/task-redirect", commit: "a41c9e2" });
    expect(model.planned).not.toContain("Prompt and plan");
    expect(model.planned).not.toContain("Check commands, exit codes and durations");
    expect(model.planned).not.toContain("What was not verified");
    expect(model.planned.join(" ")).toMatch(/Plan/);
    expect(model.planned.join(" ")).toMatch(/URL/);
    expect(model.planned.join(" ")).toMatch(/Signed proof/);
  });

  it("keeps today's Planned marks when there is no run read", () => {
    const plain = runPageModel(taskDetail(), NOW);
    expect(plain.hasRun).toBe(false);
    expect(plain.prompt).toBeNull();
    expect(plain.checkList).toBeNull();
    expect(plain.notVerified).toBeNull();
    expect(plain.commit).toBeNull();
    expect(plain.pull).toBeNull();
    expect(plain.planned).toEqual(["Prompt and plan", "Check commands, exit codes and durations", "What was not verified", "Signed proof and verify"]);
  });

  it("says why a value was hidden or the brief was cut, the notes the frames use", () => {
    expect(runPageModel(taskDetail(), NOW, runPage({ prompt: { text: "Use [hidden: key] to sign in.", hidden: true, truncated: false } })).prompt?.note)
      .toBe("Your words. A value was hidden because it looked like a key.");
    expect(runPageModel(taskDetail(), NOW, runPage({ prompt: { text: "A long brief…", hidden: false, truncated: true } })).prompt?.note)
      .toBe("Your words. The brief was cut at this page’s length.");
    expect(runPageModel(taskDetail(), NOW, runPage({ prompt: { text: "Both.", hidden: true, truncated: true } })).prompt?.note)
      .toMatch(/hidden.*cut/);
  });
});

describe("a check the read withheld", () => {
  it("says so for a task that is not final, never blank", () => {
    const model = runPageModel(taskDetail(), NOW, runningRunPage());
    expect(model.checkList).toHaveLength(2);
    for (const check of model.checkList!) {
      expect(check.withheld).toBe(true);
      expect(check.command).toBe(WITHHELD);
      expect(check.proves).toBe(WITHHELD);
      expect(check.outcome).toBeTruthy();
      expect(check.exitCode).not.toBeNull();
      expect(check.duration).toBeTruthy();
    }
  });

  it("uses finished as the signal, so an older service that still sends a running check's text is not shown", () => {
    const model = runPageModel(taskDetail(), NOW, runningRunPage({
      checks: [{ label: "pnpm test auth", outcome: "fail", exit_code: 1, duration_ms: 22000, proves: "next=//evil.com is rejected.", commit: "a41c9e2" }],
    }));
    expect(model.checkList![0]!.withheld).toBe(true);
    expect(model.checkList![0]!.command).toBe(WITHHELD);
    expect(model.checkList![0]!.proves).toBe(WITHHELD);
    expect(runPageHtml(runPageModel(taskDetail(), NOW, runningRunPage({ checks: [
      { label: "pnpm test auth", outcome: "fail", exit_code: 1, duration_ms: 22000, proves: "next=//evil.com is rejected.", commit: "a41c9e2" },
    ] })))).not.toContain("pnpm test auth");
  });

  it("shows the texts of a final task", () => {
    const model = runPageModel(taskDetail(), NOW, runPage());
    expect(model.checkList!.every((one) => !one.withheld)).toBe(true);
    expect(model.checkList![0]!.command).toBe("pnpm lint");
  });

  it("tolerates empty strings in a final read without inventing a value", () => {
    const model = runPageModel(taskDetail(), NOW, runPage({ checks: [{ label: "", outcome: "pass", exit_code: null, duration_ms: 0, proves: "", commit: "" }] }));
    expect(model.checkList![0]!.command).toBe("Not recorded.");
    expect(model.checkList![0]!.proves).toBe("Not recorded.");
    expect(model.checkList![0]!.commit).toBeNull();
  });
});

describe("the static page with a run read", () => {
  const html = runPageHtml(runPageModel(taskDetail(), NOW, runPage()));
  it("carries the prompt, each check, what was not verified, the commit and the pull request", () => {
    for (const text of ["Return people to the page they came from. Add tests.", "pnpm lint", "pnpm test auth", "exit 0", "4 s", "The changed files follow the style rules.", "The mobile app’s redirect: no check covers it.", "a41c9e2", "#142", "crew/task-redirect"]) {
      expect(html).toContain(text);
    }
    expect(html).not.toContain("Counts only; the commands are Planned.");
    expect(html).toContain("Planned");
  });
  it("keeps a hostile check command and proves line inert text", () => {
    const hostile = runPageHtml(runPageModel(taskDetail(), NOW, runPage({
      checks: [{ label: `<script>alert(9)</script> ${HOSTILE}`, outcome: "pass", exit_code: 0, duration_ms: 1, proves: `<img src=x onerror=alert(8)>`, commit: "a41c9e2" }],
    })));
    expect(hostile).not.toMatch(/<script/i);
    expect(hostile).not.toMatch(/<img/i);
    expect(hostile).toContain("&lt;script&gt;alert(9)&lt;/script&gt;");
    expect(hostile).toContain("&lt;img src=x onerror=alert(8)&gt;");
    expect(hostile).toContain("default-src 'none'");
  });
  it("says a withheld check is withheld, not blank", () => {
    expect(runPageHtml(runPageModel(taskDetail(), NOW, runningRunPage()))).toContain(WITHHELD);
  });
  it("says a value was hidden and the brief was cut, as the frames do", () => {
    const noted = runPageHtml(runPageModel(taskDetail(), NOW, runPage({ prompt: { text: "A short brief.", hidden: true, truncated: true } })));
    expect(noted).toContain("A value was hidden because it looked like a key");
    expect(noted).toContain("cut at this page’s length");
  });
});

describe("the PR comment with a run read", () => {
  const model = runPageModel(taskDetail(), NOW, runPage());
  const md = runPageMarkdown(model, 142);
  it("carries the prompt, the checks, what was not verified, the commit and the pull request", () => {
    for (const text of ["Return people to the page they came from. Add tests.", "pnpm lint", "pnpm test auth", "pass", "fail", "exit 0", "4 s", "The changed files follow the style rules.", "The mobile app’s redirect", "a41c9e2", "#142", "crew/task\\-redirect"]) {
      expect(md).toContain(text);
    }
  });
  it("escapes a hostile command and proves line instead of starting a link, mention or HTML", () => {
    const hostile = runPageMarkdown(runPageModel(taskDetail(), NOW, runPage({
      checks: [{ label: `<script>alert(9)</script> @octocat [x](https://evil.example)`, outcome: "pass", exit_code: 0, duration_ms: 1, proves: "#123 www.evil.example", commit: "a41c9e2" }],
    })), null);
    expect(hostile).not.toMatch(/(^|[^\\])</);
    expect(hostile).not.toMatch(/@octocat/);
    expect(hostile).not.toContain("](");
    expect(hostile).not.toMatch(/https?:\/\//);
    expect(hostile).not.toMatch(/#123/);
    expect(hostile).not.toMatch(/www\./);
  });
  it("says a withheld check is withheld, not blank", () => {
    expect(runPageMarkdown(runPageModel(taskDetail(), NOW, runningRunPage()), null)).toContain(WITHHELD);
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
    const many = Array.from({ length: 200 }, (_, n) => ({ id: `f${n}`, lane: "code", round: 1, severity: "low", path: `src/file${n}.ts`, line: n, claim: "x".repeat(300), evidence: "", check: "confirmed", status: "open", blocks: true }));
    const big = runPageMarkdown(runPageModel(taskDetail({ findings: many }), NOW), null);
    expect(big.length).toBeLessThanOrEqual(RUN_COMMENT_CAP);
    expect(big).toMatch(/more/);
  });
});
