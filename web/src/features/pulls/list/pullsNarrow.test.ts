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
  // The drawn steps are at list widths 820, 760, 710 and 590 px (queries at 819, 759, 709 and 589): the list reads
  // its own width, the window minus 56 px and, from 900 up, the docked sidebar's 224 px.
  it("has the four drawn steps", () => expect(steps.map((step) => step.width)).toEqual([819, 759, 709, 589]));

  it("draws the tracks in the width table: Checks 132, Reviewers 118, Your review 130, Comments and Updated 44", () => {
    expect(css).toMatch(/--c-rev: 118px;\s*--c-yours: 130px;/);
    expect(css).toMatch(/--pulls-cols: 24px minmax\(0, 1fr\) 132px var\(--c-rev\) var\(--c-yours\) 44px 44px;/);
  });

  it("drops Updated at 820, Comments at 760, and at 710 keeps the Checks icon 36 px with Reviewers 96 px", () => {
    const at = (width: number) => /--pulls-cols: ([^;]+);/.exec(steps.find((one) => one.width === width)!.body)![1];
    expect(at(819)).toBe("24px minmax(0, 1fr) 132px var(--c-rev) var(--c-yours) 44px");
    expect(at(759)).toBe("24px minmax(0, 1fr) 132px var(--c-rev) var(--c-yours)");
    expect(at(709)).toBe("24px minmax(0, 1fr) 36px var(--c-rev-n) var(--c-yours-n)");
    expect(at(589)).toBe("24px minmax(0, 1fr) 36px var(--c-yours-n)");
    expect(css).toMatch(/--c-rev-n: 96px;\s*--c-yours-n: 130px;/);
    expect(steps.find((one) => one.width === 819)!.body).toContain(".h-age");
    expect(steps.find((one) => one.width === 759)!.body).toContain(".h-comments");
    expect(steps.find((one) => one.width === 709)!.body).toContain(".h-checks-icon");
    expect(steps.find((one) => one.width === 589)!.body).toContain(".h-reviewers");
  });

  it("sends the last column step to the phone list under 600 px of window, where the boxes become cards", () => {
    const phone = css.slice(css.indexOf("@media (max-width: 599px)"));
    expect(phone).toMatch(/--pulls-cols: minmax\(0, 1fr\);/);
    expect(phone).toContain(".pull-phone-line");
    expect(phone).toContain(".pull-row-actions");
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
