import { spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

// Vitest runs from the web folder.
const script = join(process.cwd(), "scripts", "assert-no-previews.mjs");

function run(files) {
  const dir = mkdtempSync(join(tmpdir(), "no-previews-"));
  mkdirSync(join(dir, "assets"));
  for (const [name, text] of Object.entries(files)) writeFileSync(join(dir, "assets", name), text);
  return spawnSync(process.execPath, [script, dir], { encoding: "utf8" });
}

describe("assert-no-previews", () => {
  it("passes a bundle with no preview screen", () => {
    expect(run({ "index.js": "console.log('app')" }).status).toBe(0);
  });

  it("passes the real Crew screens, which ship behind the service's own flag", () => {
    const real = `"Crew is not connected to this app yet";"/api/crew/preview";.ap-chip{display:inline-flex}`;
    expect(run({ "crew.js": real }).status).toBe(0);
  });

  it.each([
    ["the Crew hash route", `const r=/^#\\/preview\\/crew\\/([a-z-]+)$/;`],
    ["the Crew route as text", `location.hash="#/preview/crew/review-queue"`],
    ["a Crew frame id", `["mergequeue-narrow-panel"]`],
    ["a Crew screen class", `.crew-apq{display:grid}`],
    ["a platform screen", `"Sign in to Orbyn"`],
  ])("fails a production bundle holding %s", (_name, text) => {
    const result = run({ "chunk.js": text });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("found in production output");
  });
});
