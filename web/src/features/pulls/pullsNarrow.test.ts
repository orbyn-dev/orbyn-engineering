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
  it("has the four drawn steps", () => expect(steps.map((step) => step.width)).toEqual([859, 779, 699, 559]));

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
      const tracks = /--pulls-cols: ([^;]+);/.exec(step.body)![1]!.trim().split(/\s+(?![^(]*\))/).length;
      expect(tracks).toBe(FULL.length - hidden.size);
    }
  });
});
