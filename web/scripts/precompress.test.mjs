import { brotliDecompressSync, gunzipSync } from "node:zlib";
import { existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { MINIMUM_BYTES, precompress } from "./precompress.mjs";

// Vitest runs from the web folder.
function bundle(files) {
  const dir = mkdtempSync(join(tmpdir(), "precompress-"));
  for (const [name, bytes] of Object.entries(files)) {
    const path = join(dir, name);
    mkdirSync(join(path, ".."), { recursive: true });
    writeFileSync(path, bytes);
  }
  return dir;
}

const body = (lines) => `export const marker = 1;\n`.repeat(lines);

describe("precompress", () => {
  it("writes a decodable sibling for both codings and reports the totals", () => {
    const source = body(40);
    const dir = bundle({ "assets/index-abc.js": source });
    const totals = precompress(dir);
    const identity = readFileSync(join(dir, "assets/index-abc.js"));
    const brotli = readFileSync(join(dir, "assets/index-abc.js.br"));
    const gzip = readFileSync(join(dir, "assets/index-abc.js.gz"));
    expect(brotliDecompressSync(brotli).toString()).toBe(source);
    expect(gunzipSync(gzip).toString()).toBe(source);
    // The point of the sibling: fewer bytes on the wire than the file it replaces.
    expect(brotli.length).toBeLessThan(identity.length);
    expect(gzip.length).toBeLessThan(identity.length);
    expect(totals).toEqual({ files: 1, identity: identity.length, br: brotli.length, gz: gzip.length });
  });

  it("leaves files that are already compressed, small or incompressible alone", () => {
    const dir = bundle({
      "assets/app.js": body(40),
      "assets/photo.png": randomBytes(MINIMUM_BYTES * 2),
      "assets/font.woff2": randomBytes(MINIMUM_BYTES * 2),
      "assets/tiny.css": "body{color:red}",
      // Random bytes with a compressible extension: both codings grow the file.
      "assets/random.js": randomBytes(MINIMUM_BYTES * 2),
    });
    precompress(dir);
    for (const name of ["assets/photo.png", "assets/font.woff2", "assets/tiny.css", "assets/random.js"]) {
      for (const suffix of ["br", "gz"]) {
        expect(existsSync(join(dir, `${name}.${suffix}`)), `${name}.${suffix}`).toBe(false);
      }
    }
    expect(existsSync(join(dir, "assets/app.js.br"))).toBe(true);
  });

  it("removes a sibling that an earlier build left for a file it no longer encodes", () => {
    const dir = bundle({
      "assets/photo.png": randomBytes(MINIMUM_BYTES * 2),
      "assets/photo.png.br": "stale",
      "assets/old.css": "body{color:red}",
      "assets/old.css.gz": "stale",
    });
    precompress(dir);
    expect(existsSync(join(dir, "assets/photo.png.br"))).toBe(false);
    expect(existsSync(join(dir, "assets/old.css.gz"))).toBe(false);
  });

  it("is idempotent and walks nested directories", () => {
    const dir = bundle({ "assets/fonts/index.css": body(40), "index.html": `<title>app</title>${"x".repeat(2000)}` });
    const first = precompress(dir);
    const size = readFileSync(join(dir, "assets/fonts/index.css.br")).length;
    const second = precompress(dir);
    expect(second).toEqual(first);
    expect(readFileSync(join(dir, "assets/fonts/index.css.br")).length).toBe(size);
    expect(existsSync(join(dir, "index.html.br"))).toBe(true);
  });
});
