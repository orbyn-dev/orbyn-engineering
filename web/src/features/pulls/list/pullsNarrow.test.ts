import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

/** jsdom has no container queries, so the stylesheet is read: each step must keep header, rows and tracks the same. */
const css = readFileSync(join(process.cwd(), "src/features/pulls/pulls.css"), "utf8");
const FULL = ["lead", "title", "checks", "reviewers", "yours", "comments", "age"];
const HIDES: Record<string, string> = { age: "h-age", comments: "h-comments", reviewers: "h-reviewers" };
const CELL: Record<string, string> = { comments: "pull-comments-cell", reviewers: "pull-reviewers-cell" };

describe("the narrow list", () => {
  const steps = [...css.matchAll(/@container pulls \(max-width: (\d+)px\) \{([\s\S]*?)\n\}\n/g)].map((one) => ({ width: Number(one[1]), body: one[2]! }));
  it("has the four drawn steps", () => expect(steps.map((step) => step.width)).toEqual([759, 699, 639, 579]));

  it("draws the 900 px list: Checks an icon, Reviewers 96 px, Your review 130 px, Updated and Comments gone", () => {
    const step = steps.find((one) => one.width === 639)!;
    expect(/--pulls-cols: ([^;]+);/.exec(step.body)![1]).toBe("24px minmax(0, 1fr) 36px var(--c-rev-n) var(--c-yours-n)");
    expect(css).toMatch(/--c-rev-n: 96px;\s*--c-yours-n: 130px;/);
    expect(steps.find((one) => one.width === 699)!.body).toContain(".h-comments");
    expect(steps.find((one) => one.width === 759)!.body).toContain(".h-age");
  });

  it("keeps one track per visible column at every step, and hides the header and the row cell together", () => {
    const hidden = new Set<string>();
    for (const step of steps) {
      for (const [column, cls] of Object.entries(HIDES)) {
        if (step.body.includes(`.${cls}`)) hidden.add(column);
      }
      for (const [column, cls] of Object.entries(CELL)) {
        // Whatever drops from the header drops from the rows in the same step, or already has.
        if (hidden.has(column)) expect(step.body.includes(`.${cls}`) || steps.some((one) => one.width >= step.width && one.body.includes(`.${cls}`))).toBe(true);
      }
      // The tracks are set on the grids (the header and the rows): a container query never styles its own container.
      expect(step.body).toMatch(/\.pulls-head,\s*\.pull-rows > li \{\s*--pulls-cols/);
      const tracks = /--pulls-cols: ([^;]+);/.exec(step.body)![1]!.trim().split(/\s+(?![^(]*\))/).length;
      expect(tracks).toBe(FULL.length - hidden.size);
    }
  });
});
