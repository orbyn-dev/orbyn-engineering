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

  it("passes the real command blocks, which ship hidden behind a client that is null in production", () => {
    expect(run({ "term.js": `.cb-block{display:flex}.cb-head{position:sticky}"Commands Orbyn runs appear here as blocks"` }).status).toBe(0);
  });

  it("passes the real profile screens, which ship hidden behind a client that answers 404", () => {
    expect(run({ "prf.js": `.prf-row{display:grid}"Execution profiles";"/api/profiles"` }).status).toBe(0);
  });

  it("passes the real library panel, which ships hidden behind a client that answers 404", () => {
    expect(run({ "lib.js": `.lib-row{display:flex}"Search the Library";"/api/library"` }).status).toBe(0);
  });

  it("passes the real attention item, which ships hidden behind a client that is null in production", () => {
    expect(run({ "thread.js": `.ta-item{display:flex}"Terminal agents waiting for you"` }).status).toBe(0);
  });

  it("passes the real Crew screens, which ship behind the service's own flag", () => {
    const real = `"Crew is not connected to this app yet";"/api/crew/preview";.ap-chip{display:inline-flex};"/api/crew/trust";.cx-dash{display:flex}`;
    expect(run({ "crew.js": real }).status).toBe(0);
  });

  it.each([
    ["the Crew hash route", `const r=/^#\\/preview\\/crew\\/([a-z-]+)$/;`],
    ["the Crew route as text", `location.hash="#/preview/crew/review-queue"`],
    ["a Crew frame id", `["mergequeue-narrow-panel"]`],
    ["a Crew screen class", `.crew-apq{display:grid}`],
    ["the Issues hash route", `location.hash.startsWith("#/preview/issues")`],
    ["the Issues mock", `module.createIssuesMock({ delayMs: 400 })`],
    ["a Crew dashboard frame id", `["dash-narrow-window"]`],
    ["the Crew dashboard sample client", `module.createCrewDashboardMock({delayMs:300})`],
    ["a platform screen", `"Sign in to Orbyn"`],
    ["the Cursor thread preview route", `location.hash.startsWith("#/preview/cursor-thread")`],
    ["the Cursor thread mock", `createCursorThreadMock({delayMs:300})`],
    ["the Repositories preview route", `location.hash.startsWith("#/preview/repos")`],
    ["the Repositories mock", `module.createReposMock({delayMs:300})`],
    ["the Devices preview route", `location.hash.startsWith("#/preview/devices")`],
    ["the Devices mock", `module.createDevicesMock({delayMs:300})`],
    ["the attachments preview route", `location.hash.startsWith("#/preview/attachments")`],
    ["the photo conversion mock", `module.createPhotoConversionMock({delayMs:1200})`],
    ["the transcripts mock", `module.createTranscriptsMock({delayMs:800})`],
    ["the earlier messages preview route", `location.hash.startsWith("#/preview/earlier")`],
    ["the earlier messages mock", `module.createEarlierMock({delayMs:400})`],
    ["the command blocks preview route", `location.hash.startsWith("#/preview/command-blocks")`],
    ["the command blocks preview route as a regex source", `const r=/^#\\/preview\\/command-blocks$/;`],
    ["the command blocks mock", `module.createCommandBlocksMock({delayMs:250})`],
    ["the command blocks preview page", `.cbp-drawer{display:flex}`],
    ["the profiles preview route", `location.hash.startsWith("#/preview/profiles")`],
    ["the profiles mock", `module.createProfilesMock({delayMs:300})`],
    ["the library preview route", `location.hash.startsWith("#/preview/library")`],
    ["the library mock", `module.createLibraryMock({delayMs:300})`],
    ["the attention preview route", `location.hash.startsWith("#/preview/attention")`],
    ["the attention preview route as a regex source", `const r=/^#\\/preview\\/attention$/;`],
    ["the attention mock", `module.createAttentionMock({scenario:"many"})`],
    ["the attention preview page", `.atp-thread{max-width:340px}`],
  ])("fails a production bundle holding %s", (_name, text) => {
    const result = run({ "chunk.js": text });
    expect(result.status).toBe(1);
    expect(result.stderr).toContain("found in production output");
  });
});
