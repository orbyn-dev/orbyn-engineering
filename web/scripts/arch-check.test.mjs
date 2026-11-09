import { describe, expect, it } from "vitest";
import { analyze, compare, stripComments } from "./arch-check.mjs";

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
const bare = (r) => ({ ...r, unresolved: undefined });

describe("arch-check", () => {
  it("passes clean code and index imports between features", () => {
    const r = analyze(base());
    expect(bare(r)).toEqual({ ...empty, unresolved: undefined });
    expect(compare(r, empty).fresh).toEqual([]);
  });

  it("flags shared code importing a feature", () => {
    const s = base().set("ui/Bad.tsx", `import { a } from "../features/a/A";\n`);
    expect(analyze(s).direction).toEqual(["ui/Bad.tsx -> features/a/A.tsx"]);
  });

  it("flags a feature importing app, and api importing ui, lib or state", () => {
    const s = base()
      .set("app/shortcuts.ts", "export const k = 1;\n")
      .set("features/a/K.tsx", `import { k } from "../../app/shortcuts";\n`)
      .set("api/client.ts", `import { u } from "../lib/util";\n`);
    const r = analyze(s);
    expect(r.direction).toEqual(["api/client.ts -> lib/util.ts", "features/a/K.tsx -> app/shortcuts.ts"]);
  });

  it("ignores paths in comments and warns on unresolved specifiers", () => {
    const s = base().set("ui/C.tsx", `// import x from "../features/a/A";\n/* import y from "../features/b" */\nimport "./missing";\nimport "./c.css";\nconst u = "http://x";\n`);
    const r = analyze(s);
    expect(r.direction).toEqual([]);
    expect(r.unresolved).toEqual(["ui/C.tsx: ./missing"]);
    expect(stripComments(`a // b\nc`)).toBe("a \nc");
  });

  it("fails a baselined item that no longer exists, and only notes a shrink", () => {
    const old = { ...empty, deep: ["x -> y"], lines: { "lib/long.ts": 700 } };
    const r = compare(analyze(base().set("lib/long.ts", lines(650))), old);
    expect(r.stale).toEqual(["deep: x -> y"]);
    expect(r.shrunk).toHaveLength(1);
    expect(r.fresh).toEqual([]);
    expect(compare(analyze(base()), old).stale).toHaveLength(2);
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
