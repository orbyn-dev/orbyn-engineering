import { describe, expect, it } from "vitest";
import { analyze, compare } from "./arch-check.mjs";

const lines = (n) => `${"x\n".repeat(n)}`;
const base = () =>
  new Map([
    ["features/a/index.ts", `export * from "./A";\n`],
    ["features/a/A.tsx", `import { b } from "../b";\nexport const a = b;\n`],
    ["features/b/index.ts", `export const b = 1;\n`],
    ["ui/Button.tsx", `export const Button = 1;\n`],
    ["lib/util.ts", `import { Button } from "../ui/Button";\nexport const u = Button;\n`],
  ]);
const empty = { direction: [], deep: [], cycle: [], lines: {}, css: {} };

describe("arch-check", () => {
  it("passes clean code and index imports between features", () => {
    const r = analyze(base());
    expect(r).toEqual(empty);
    expect(compare(r, empty).fresh).toEqual([]);
  });

  it("flags shared code importing a feature", () => {
    const s = base().set("ui/Bad.tsx", `import { a } from "../features/a/A";\n`);
    expect(analyze(s).direction).toEqual(["ui/Bad.tsx -> features/a/A.tsx"]);
  });

  it("flags a deep import into another feature but not its index", () => {
    const s = base().set("features/a/C.tsx", `import { x } from "../b/Inner";\n`).set("features/b/Inner.ts", "export const x = 1;\n");
    expect(analyze(s).deep).toEqual(["features/a/C.tsx -> features/b/Inner.ts"]);
  });

  it("flags edges inside a new import cycle", () => {
    const s = base().set("lib/p.ts", `import "./q";\n`).set("lib/q.ts", `import "./p";\n`);
    expect(analyze(s).cycle).toEqual(["lib/p.ts -> lib/q.ts", "lib/q.ts -> lib/p.ts"]);
  });

  it("fails a new violation and passes a baselined one", () => {
    const s = base().set("ui/Bad.tsx", `import { a } from "../features/a/A";\n`);
    const found = analyze(s);
    expect(compare(found, empty).fresh).toEqual(["direction: ui/Bad.tsx -> features/a/A.tsx"]);
    expect(compare(found, found).fresh).toEqual([]);
    expect(compare(empty, found).stale).toHaveLength(1);
  });

  it("ratchets long files and ignores tests, generated code and short files", () => {
    const s = base()
      .set("lib/long.ts", lines(601))
      .set("lib/ok.ts", lines(600))
      .set("lib/long.test.ts", lines(900))
      .set("generated/big.ts", lines(9000))
      .set("styles/a.css", lines(1501));
    const found = analyze(s);
    expect(found.lines).toEqual({ "lib/long.ts": 601 });
    expect(found.css).toEqual({ "styles/a.css": 1501 });
    expect(compare(found, empty).fresh).toHaveLength(2);
    expect(compare(found, found).fresh).toEqual([]);
    const grown = analyze(s.set("lib/long.ts", lines(650)));
    expect(compare(grown, found).fresh[0]).toContain("grew from 601 to 650");
  });
});
