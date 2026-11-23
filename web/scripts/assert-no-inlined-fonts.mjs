// Fails the build if a font reached the output as a `data:` URI. The service serves this build with
// `font-src 'self'` (crates/orbynd/src/routes/ui.rs), so the browser blocks the font and the text
// falls back to a system font. web/vite.config.ts keeps every font a file under /assets/.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

// The font types a bundler writes: font/woff2, application/font-woff, application/x-font-ttf and
// the older application/vnd.ms-fontobject. The quotes, spaces and case around the URI vary.
const inlineFont = /url\(\s*['"]?data:(?:font\/|application\/(?:x-)?font|application\/vnd\.ms-fontobject)/gi;
const dir = process.argv[2] ?? "dist";
const walk = (path) =>
  readdirSync(path).flatMap((name) => {
    const full = join(path, name);
    return statSync(full).isDirectory() ? walk(full) : [full];
  });

const offenders = walk(dir)
  .filter((path) => /\.(js|css|html)$/.test(path))
  .flatMap((path) => {
    const count = [...readFileSync(path, "utf8").matchAll(inlineFont)].length;
    return count === 0 ? [] : [`${path} (${count})`];
  });

if (offenders.length > 0) {
  console.error(`Fonts inlined as data: URIs found in production output: ${offenders.join(", ")}`);
  process.exit(1);
}
console.log("No inlined fonts in production output.");
