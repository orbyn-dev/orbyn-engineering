import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
// @ts-expect-error plain modules shared with the render script
import { FRAMES } from "./catalog.mjs";
// @ts-expect-error plain modules shared with the render script
import { MARKER, askForFrames, buildComment } from "./comment.mjs";
// @ts-expect-error plain modules shared with the render script
import { MAX_FRAMES, framesBuilt, selectFrames } from "./frames.mjs";

type Built = { line: "missing" | "empty" | "none" | "ids"; ids: string[]; version: string | null };
const built = framesBuilt as (body: string) => Built;
const pick = selectFrames as (ids: string[], known: Record<string, unknown>, max?: number) => { use: string[]; unknown: string[]; over: string[] };
const comment = buildComment as (p: Record<string, unknown>) => string;
const ask = askForFrames as (reason: string) => string;
const catalog = FRAMES as Record<string, { route: string; scenario?: string; pre?: unknown[]; steps?: { do: string }[] }>;

const web = process.cwd();
const repo = join(web, "..");
const workflow = (name: string) => readFileSync(join(repo, ".github/workflows", name), "utf8");

describe("the Frames built line", () => {
  it("reads the frame ids and the design version", () => {
    expect(built("## Summary\n\n- x\n\nFrames built: pr-list-failed, pr-detail-running @ v56\n\n## Testing")).toEqual({
      line: "ids",
      ids: ["pr-list-failed", "pr-detail-running"],
      version: "v56",
    });
  });

  it("keeps an id that has an @ of its own, and a list without a version", () => {
    expect(built("Frames built: a@1 b")).toEqual({ line: "ids", ids: ["a@1", "b"], version: null });
    expect(built("- Frames built: `shell-home`, `shell-home`  @  v50")).toEqual({ line: "ids", ids: ["shell-home"], version: "v50" });
  });

  it("says none when the pull request draws nothing", () => {
    expect(built("Frames built: none")).toMatchObject({ line: "none", ids: [] });
    expect(built("Frames built: n/a @ v56")).toMatchObject({ line: "none" });
  });

  it("asks when the line is missing or empty, and the template's own example in a comment does not count", () => {
    expect(built("## Summary\n- x").line).toBe("missing");
    expect(built(null as unknown as string).line).toBe("missing");
    expect(built("Frames built:").line).toBe("empty");
    expect(built("<!-- Frames built: shell-home @ v56 -->\n\nFrames built:\n").line).toBe("empty");
  });
});

describe("which frames are drawn", () => {
  const known = Object.fromEntries(Array.from({ length: 20 }, (_, i) => [`f${i}`, {}]));

  it("draws the frames the catalog knows, names the ones it does not, and stops at 12", () => {
    const ids = [...Array.from({ length: 14 }, (_, i) => `f${i}`), "nope"];
    const r = pick(ids, known);
    expect(MAX_FRAMES).toBe(12);
    expect(r.use).toHaveLength(12);
    expect(r.over).toEqual(["f12", "f13"]);
    expect(r.unknown).toEqual(["nope"]);
  });
});

describe("the catalog", () => {
  it("only names routes the app has, and steps the shooter can run", () => {
    const verbs = ["click", "fill", "press", "route", "offline", "wait"];
    for (const [id, frame] of Object.entries(catalog)) {
      expect(frame.route, id).toMatch(/^#\/(pulls|threads|settings|new)?/);
      for (const step of frame.steps ?? []) expect(verbs, id).toContain(step.do);
    }
    expect(Object.keys(catalog).length).toBeGreaterThan(20);
  });

  it("only uses mock scenarios that exist", () => {
    const api = readFileSync(join(web, "mock/api.ts"), "utf8");
    for (const [id, frame] of Object.entries(catalog)) {
      if (frame.scenario) expect(api, id).toContain(`"${frame.scenario}"`);
    }
  });

  it("is documented: every id is listed in docs/match-kit.md", () => {
    const doc = readFileSync(join(repo, "docs/match-kit.md"), "utf8");
    for (const id of Object.keys(catalog)) expect(doc, id).toContain(`\`${id}\``);
  });
});

describe("the comment", () => {
  const url = (rel: string) => `https://x/${rel}?raw=true`;
  const base = { failed: [], unknown: [], over: [], version: "v56", url, runUrl: "https://run", sha: "abcdef1234", error: null };

  it("shows Light first and folds Dark and Dim, with the version and the run link", () => {
    const text = comment({ ...base, units: [{ id: "shell-home", images: { light: "shell-home/light.png", dark: "shell-home/dark.png", dim: "shell-home/dim.png" } }] });
    expect(text.startsWith(MARKER)).toBe(true);
    expect(text).toContain("Frames built @ v56: 1 drawn from the dev-only mock app (fake data, no account), commit `abcdef1`");
    expect(text.indexOf("shell-home/light.png?raw=true")).toBeLessThan(text.indexOf("<details>"));
    expect(text.indexOf("<details>")).toBeLessThan(text.indexOf("shell-home/dark.png?raw=true"));
    expect(text).toContain("shell-home/dim.png?raw=true");
    expect(text).toContain("[this workflow run](https://run)");
  });

  it("says so when a frame could not be drawn, an id is unknown, or the cap was reached", () => {
    const text = comment({ ...base, units: [], failed: ["a"], unknown: ["b"], over: ["c"] });
    expect(text).toContain("Could not draw: `a`");
    expect(text).toContain("Not screens the preview knows: `b`");
    expect(text).toContain("Over the limit of 12, not drawn: `c`");
  });

  it("says so when nothing could be drawn, and never blocks", () => {
    const text = comment({ ...base, units: [], error: "no image was written" });
    expect(text).toContain("The preview could not be built for `abcdef1`: no image was written");
    expect(text).toContain("Nothing is blocked.");
  });

  it("asks for the line without blocking anything", () => {
    expect(ask("missing")).toContain("has no **Frames built** line");
    expect(ask("empty")).toContain("The **Frames built** line is empty");
    expect(ask("missing")).toContain("nothing is blocked");
    expect(ask("missing").startsWith(MARKER)).toBe(true);
  });
});

describe("the workflows", () => {
  const names = ["ui-preview.yml", "ui-preview-cleanup.yml"];

  it("run for pull requests of this repository only, never from a fork, and take no secret", () => {
    for (const name of names) {
      const text = workflow(name);
      expect(text, name).toContain("github.event.pull_request.head.repo.full_name == github.repository");
      expect(text, name).not.toContain("pull_request_target");
      expect(text, name).not.toMatch(/secrets\./);
    }
  });

  it("pin every action to a commit", () => {
    for (const name of names) {
      const uses = [...workflow(name).matchAll(/^\s*-?\s*uses:\s*(\S+)/gm)].map((m) => m[1]!);
      for (const u of uses) expect(u, name).toMatch(/@[0-9a-f]{40}$/);
    }
    expect([...workflow("ui-preview.yml").matchAll(/^\s*-?\s*uses:\s*(\S+)/gm)].length).toBeGreaterThanOrEqual(3);
  });

  it("draw our own app only: nothing reads or publishes the private designs", () => {
    const text = workflow("ui-preview.yml");
    expect(text).not.toMatch(/orbyn-designs|ORBYN_DESIGNS_DIR|ORBYN_DESIGN_FILE|engineer-designs/);
    expect(text).toContain("tools/pr-preview/render.mjs");
    for (const file of readdirSync(join(web, "tools/pr-preview")).filter((f) => f.endsWith(".mjs"))) {
      const source = readFileSync(join(web, "tools/pr-preview", file), "utf8").replace(/\/\/.*$/gm, "");
      expect(source, file).not.toMatch(/orbyn-designs|engineer-designs|ORBYN_DESIGNS_DIR/);
    }
  });

  it("is informational: a failure comments, and nothing makes it a required check", () => {
    const text = workflow("ui-preview.yml");
    expect(text).toContain("Say so when the preview failed");
    expect(text).toContain("Nothing is blocked");
    expect(text).not.toMatch(/^\s*required:/m);
  });

  it("run for a change to web/ and the template, and the cleanup runs when a pull request closes", () => {
    expect(workflow("ui-preview.yml")).toMatch(/paths:\s*\n\s*- web\/\*\*/);
    expect(workflow("ui-preview-cleanup.yml")).toContain("types: [closed]");
  });
});

describe("the pull request template", () => {
  const template = readFileSync(join(repo, ".github/pull_request_template.md"), "utf8");

  it("has the Frames built line, empty, so a bot can ask for it, and the AI usage section stays whole", () => {
    expect(built(template).line).toBe("empty");
    const bare = template.replace(/<!--[\s\S]*?-->/g, "");
    const section = /^## AI usage\s*$([\s\S]*?)(?=^## |$(?![\s\S]))/m.exec(bare)![1]!;
    expect(section).not.toContain("Frames built");
    for (const field of ["Harness and version", "Models", "Effort", "Human role", "Sources"]) expect(section).toContain(`- ${field}:`);
  });
});

describe("dev only", () => {
  const walk = (dir: string): string[] =>
    readdirSync(dir, { withFileTypes: true }).flatMap((entry) => (entry.isDirectory() ? walk(join(dir, entry.name)) : [join(dir, entry.name)]));

  it("nothing in the app imports the preview tools", () => {
    for (const file of walk(join(web, "src")).filter((f) => /\.(ts|tsx)$/.test(f))) {
      expect(readFileSync(file, "utf8"), file).not.toMatch(/tools\/pr-preview/);
    }
  });
});
