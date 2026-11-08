// DEVELOPMENT ONLY. Draws the frames a pull request says it built, ours only, in Light, Dark and Dim, and writes the comment.
//
//   node tools/pr-preview/render.mjs --body <file> --out <dir> [--url <prefix>] [--run-url <url>] [--sha <sha>]
//
// Reads the "Frames built" line of the description in <file>, drives the dev-only mock app (shoot.mjs) to each screen the catalog knows
// and writes <out>/images/<frame>/<theme>.png, <out>/manifest.json and <out>/comment.md. It never reads the private designs.
import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { FRAMES } from "./catalog.mjs";
import { askForFrames, buildComment } from "./comment.mjs";
import { framesBuilt, selectFrames } from "./frames.mjs";
import { THEMES, shoot } from "./shoot.mjs";

const web = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const flags = Object.fromEntries(process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith("--") ? [...acc, [a.slice(2), all[i + 1]]] : acc), []));
const out = resolve(String(flags.out));
const urlPrefix = String(flags.url ?? ".");
const runUrl = String(flags["run-url"] ?? "");
const sha = String(flags.sha ?? "0000000");
const body = existsSync(String(flags.body)) ? readFileSync(String(flags.body), "utf8") : "";

mkdirSync(join(out, "images"), { recursive: true });
const built = framesBuilt(body);
const done = (comment, manifest) => {
  writeFileSync(join(out, "comment.md"), comment);
  writeFileSync(join(out, "manifest.json"), JSON.stringify({ sha, ...manifest }, null, 2));
};

if (built.line === "missing" || built.line === "empty") {
  done(askForFrames(built.line), { asked: true, units: [] });
  console.log("no Frames built line: asking for one");
} else if (built.line === "none") {
  done("", { none: true, units: [] });
  console.log("Frames built: none");
} else {
  const { use, unknown, over } = selectFrames(built.ids, FRAMES);
  const units = [];
  let failed = [];
  let error = null;
  if (use.length > 0) {
    try {
      const drawn = await shoot({ web, ids: use, catalog: FRAMES, out: join(out, "raw"), log: (line) => console.error(line) });
      failed = drawn.failed;
      for (const id of use) {
        const images = {};
        for (const theme of THEMES) {
          const file = drawn.files[id]?.[theme];
          if (!file || !existsSync(file)) continue;
          const rel = `${id}/${theme}.png`;
          mkdirSync(dirname(join(out, "images", rel)), { recursive: true });
          copyFileSync(file, join(out, "images", rel));
          images[theme] = rel;
        }
        if (Object.keys(images).length > 0) units.push({ id, images });
      }
      if (units.length === 0) error = "no image was written";
    } catch (failure) {
      error = String(failure instanceof Error ? failure.message : failure).split("\n")[0];
    }
  } else if (unknown.length === 0) {
    error = "no frame was named";
  }
  done(buildComment({ units, failed, unknown, over, version: built.version, url: (rel) => `${urlPrefix.replace(/\/$/, "")}/images/${rel}?raw=true`, runUrl, sha, error }), { units, failed, unknown, over, error });
  console.log(`done: ${units.length} frames drawn, ${failed.length} failed`);
}
