import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// Vitest runs from the web folder.
const script = join(process.cwd(), "scripts", "assert-no-inlined-fonts.mjs");

function run(files) {
  const dir = mkdtempSync(join(tmpdir(), "no-inlined-fonts-"));
  mkdirSync(join(dir, "assets"));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, "assets", name), text);
  return spawnSync(process.execPath, [script, dir], { encoding: "utf8" });
}

describe("assert-no-inlined-fonts", () => {
  it("passes a bundle whose fonts are files", () => {
    const css = `@font-face{font-family:KaTeX_Main;src:url(/assets/KaTeX_Main-Regular-B22Nviop.woff2) format("woff2"),url(/assets/KaTeX_Main-Regular-Dr94JaBh.woff) format("woff")}`;
    expect(run({ "index.css": css }).status).toBe(0);
  });

  it("passes a data: image, which the policy allows", () => {
    expect(run({ "index.css": `.icon{background:url(data:image/png;base64,iVBORw0KGgo=)}` }).status).toBe(0);
  });

  it.each([
    ["a woff2", `url(data:font/woff2;base64,d09GMgABAAAA)`],
    ["a quoted woff", `url("data:font/woff;base64,d09GRgAB")`],
    ["an application font", `url(data:application/font-woff;base64,d09GRgAB)`],
    ["an x-font ttf", `url('data:application/x-font-ttf;base64,AAEAAA')`],
    ["an eot", `url(data:application/vnd.ms-fontobject;base64,AAEAAA)`],
    ["a spaced URI", `url( data:font/ttf;base64,AAEAAA )`],
  ])("fails a bundle holding %s as a data: URI", (_name, uri) => {
    const result = run({ "index.css": `@font-face{src:${uri}}` });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("Fonts inlined as data: URIs found in production output");
  });

  it("counts every inlined font in a file", () => {
    const css = `a{src:url(data:font/woff2;base64,AA)}b{src:url(data:font/woff;base64,AA)}`;
    const result = run({ "index.css": css });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("(2)");
  });
});
