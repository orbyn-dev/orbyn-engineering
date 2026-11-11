/**
 * Splits the browser suite across CI shards by measured duration, not by test count.
 *
 *   node scripts/e2e-shards.mjs <shard> <total>     print the shard's spec files, one Playwright filter per line
 *   node scripts/e2e-shards.mjs update <dir>        rewrite e2e/shard-durations.json from timing reports
 *
 * Playwright's own --shard splits by test count, which clusters the slow specs in a few shards.
 * Here each spec file goes, longest first, to the shard with the least time so far. The seconds
 * per file live in e2e/shard-durations.json. A spec missing from it (a new file) counts as the
 * median, so it still runs; a listed file that no longer exists is ignored.
 *
 * Refresh the map when shards drift apart: every CI shard uploads its Playwright JSON report as the
 * `e2e-timing-N` artifact (7 days). Download them into one directory
 * (`gh run download <run> -p 'e2e-timing-*' -D <dir>`) and run `update <dir>`.
 */
import { readdirSync, readFileSync, writeFileSync, existsSync, statSync } from "node:fs";
import { dirname, join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const web = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const mapPath = join(web, "e2e", "shard-durations.json");
// Mirrors `testIgnore` in playwright.config.ts: the perf baseline runs on purpose, not in CI.
const IGNORED = /perf-baseline/;
const posix = (path) => path.split("\\").join("/");

function specs(dir = join(web, "e2e")) {
  const found = [];
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) found.push(...specs(path));
    else if (name.endsWith(".spec.ts") && !IGNORED.test(name)) found.push(posix(relative(web, path)));
  }
  return found.sort();
}

function median(values) {
  if (values.length === 0) return 1;
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

/** Greedy longest-first: returns one { load, files } per shard. */
function assign(files, durations, total) {
  const fallback = median(files.filter((file) => file in durations).map((file) => durations[file]));
  const weight = (file) => durations[file] ?? fallback;
  const shards = Array.from({ length: total }, () => ({ load: 0, files: [] }));
  for (const file of [...files].sort((a, b) => weight(b) - weight(a) || a.localeCompare(b))) {
    const lightest = shards.reduce((best, shard) => (shard.load < best.load ? shard : best));
    lightest.files.push(file);
    lightest.load += weight(file);
  }
  return shards;
}

function* durations(suite) {
  for (const spec of suite.specs ?? []) for (const test of spec.tests) for (const result of test.results) yield result.duration;
  for (const child of suite.suites ?? []) yield* durations(child);
}

function update(dir) {
  const totals = {};
  const reports = readdirSync(dir, { recursive: true }).map(String).filter((name) => name.endsWith(".json"));
  if (reports.length === 0) throw new Error(`no .json reports under ${dir}`);
  for (const name of reports) {
    const report = JSON.parse(readFileSync(join(dir, name), "utf8"));
    for (const top of report.suites ?? []) {
      // A report names each file relative to its testDir, which is e2e/.
      const file = posix(join("e2e", top.file));
      for (const ms of durations(top)) totals[file] = (totals[file] ?? 0) + ms / 1000;
    }
  }
  const rounded = Object.fromEntries(Object.entries(totals).sort(([a], [b]) => a.localeCompare(b)).map(([file, s]) => [file, Math.round(s * 10) / 10]));
  writeFileSync(mapPath, `${JSON.stringify(rounded, null, 2)}\n`);
  console.error(`${Object.keys(rounded).length} specs, ${Math.round(Object.values(rounded).reduce((a, b) => a + b, 0))} s of test time`);
}

const [first, second] = process.argv.slice(2);
if (first === "update") {
  update(resolve(second ?? "."));
} else if (/^\d+$/.test(first ?? "") && /^\d+$/.test(second ?? "")) {
  const shard = Number(first);
  const total = Number(second);
  if (shard < 1 || shard > total) throw new Error(`shard ${shard} is outside 1..${total}`);
  const map = existsSync(mapPath) ? JSON.parse(readFileSync(mapPath, "utf8")) : {};
  const mine = assign(specs(), map, total)[shard - 1];
  console.error(`shard ${shard}/${total}: ${mine.files.length} files, about ${Math.round(mine.load)} s of test time`);
  // Playwright reads each argument as a regular expression on the file path.
  for (const file of mine.files.sort()) console.log(`/${file.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}$`);
} else {
  console.error("usage: e2e-shards.mjs <shard> <total> | update <dir>");
  process.exit(2);
}
