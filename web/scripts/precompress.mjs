// Writes a brotli and a gzip sibling beside every compressible file in the build
// output, so the service sends bytes compressed once here instead of on every
// response. Node's own zlib provides both coders, so no dependency is added.
//
// The service only offers a sibling for a type it expects to compress
// (crates/orbynd/src/routes/precompressed.rs) and falls back to the original file
// when there is none, so a directory that was never pre-compressed still works.
import { constants, brotliCompressSync, gzipSync } from "node:zlib";
import { readdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Extensions worth an encoded sibling; keep in step with `compressible` in the service. */
export const COMPRESSIBLE = new Set([".css", ".html", ".js", ".json", ".map", ".mjs", ".svg", ".txt", ".wasm"]);

/** A smaller file gains less than the sibling costs inside the bundle. */
export const MINIMUM_BYTES = 512;

const ENCODINGS = [
  {
    suffix: "br",
    // Measured over this build's 10.2 MB of eligible files: quality 9 -> 23.7% in
    // 1.7 s, 10 -> 22.3% in 6.2 s, 11 -> 21.9% in 15.1 s. Ten keeps the build quick
    // for the bytes it saves; quality 11 would add 0.4 points for 2.5x the time.
    encode: (bytes) =>
      brotliCompressSync(bytes, {
        params: {
          [constants.BROTLI_PARAM_QUALITY]: 10,
          [constants.BROTLI_PARAM_SIZE_HINT]: bytes.length,
        },
      }),
  },
  { suffix: "gz", encode: (bytes) => gzipSync(bytes, { level: 9 }) },
];

const isSibling = (path) => ENCODINGS.some(({ suffix }) => path.endsWith(`.${suffix}`));

function files(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory() ? files(path) : [path];
  });
}

const isCompressible = (path) => COMPRESSIBLE.has(extname(path).toLowerCase());

/**
 * Pre-compresses every compressible file in `directory` in place.
 *
 * A file that is too small, of a type that is already compressed, or that does not
 * get smaller keeps no sibling: any sibling left from an earlier build is removed,
 * so the service can never serve bytes that no longer match the file.
 */
export function precompress(directory, { minimumBytes = MINIMUM_BYTES } = {}) {
  const totals = { files: 0, identity: 0, br: 0, gz: 0 };
  for (const path of files(directory)) {
    if (isSibling(path)) continue;
    const bytes = readFileSync(path);
    const wanted = isCompressible(path) && bytes.length >= minimumBytes;
    for (const { suffix, encode } of ENCODINGS) {
      const sibling = `${path}.${suffix}`;
      const encoded = wanted ? encode(bytes) : undefined;
      if (encoded === undefined || encoded.length >= bytes.length) {
        rmSync(sibling, { force: true });
        continue;
      }
      writeFileSync(sibling, encoded);
      totals[suffix] += encoded.length;
    }
    if (wanted) {
      totals.files += 1;
      totals.identity += bytes.length;
    }
  }
  return totals;
}

const kilobytes = (bytes) => `${(bytes / 1024).toFixed(1)} kB`;
const share = (part, whole) => (whole === 0 ? "0%" : `${Math.round((part / whole) * 100)}%`);

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const directory = resolve(process.argv[2] ?? join(dirname(fileURLToPath(import.meta.url)), "..", "dist"));
  const totals = precompress(directory);
  console.log(
    `Pre-compressed ${totals.files} files: ${kilobytes(totals.identity)} -> `
      + `brotli ${kilobytes(totals.br)} (${share(totals.br, totals.identity)}), `
      + `gzip ${kilobytes(totals.gz)} (${share(totals.gz, totals.identity)})`,
  );
}
