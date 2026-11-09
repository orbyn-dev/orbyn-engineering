#!/usr/bin/env node
// Architecture guardrails for web/src (docs/refactor.md). Zero dependencies.
//
//   node scripts/arch-check.mjs            fail on NEW violations only
//   node scripts/arch-check.mjs --update   rewrite scripts/arch-baseline.json
//
// Rules, over hand-written files (tests, generated code and test helpers excluded):
//   direction  app -> features -> ui, lib, state -> api, one way: features never import app;
//              ui, lib, state and api never import app or a feature; api never imports ui, lib or state
//   deep       a feature imports another feature only through its index.ts (always an error)
//   cycle      no import cycle between files; an edge inside a cycle is a violation (always an error)
//   feature    no import cycle between features (the one-way order in docs/refactor.md); always an error
//   lines      TypeScript over 600 lines, CSS over 1500 lines; may not grow past baseline
// direction and lines keep their existing violations in the baseline, which may only shrink: a new
// violation or growth fails, and so does a baselined item that no longer exists (re-run --update so it
// cannot quietly return). deep and cycle have no baseline: any violation fails.
// A long file that shrinks but stays long only prints a notice; --update lowers its recorded size.
// Type-only imports count as edges. Imports inside comments are ignored. CI runs this on Ubuntu only.
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

export const LIMITS = { ts: 600, css: 1500 };
const SHARED = new Set(["ui", "lib", "state", "api"]);
const ASSET = /\.(css|json|svg|png|jpe?g|gif|webp|woff2?|ttf|txt|md)$/;
const SKIP_DIRS = new Set(["generated", "node_modules"]);
const TEST_FILE = /\.test\.(ts|tsx)$|(^|\/)test-(setup|support)\.ts$/;
const SOURCE = /\.(ts|tsx)$/;
const IMPORT = /(?:\bfrom\s*|\bimport\s*\(?\s*)["'](\.{1,2}\/[^"']*|\.{1,2})["']/g;

export function stripComments(text) {
  return text.replace(/\/\*[\s\S]*?\*\//g, "").replace(/(^|[^:"'\\])\/\/.*$/gm, "$1");
}

export function isHandWritten(rel) {
  return !rel.startsWith("generated/") && !TEST_FILE.test(rel);
}

export function layerOf(rel) {
  const [top, second] = rel.split("/");
  if (top === "features" && second && rel.split("/").length > 2) return { layer: "features", feature: second };
  return { layer: top, feature: null };
}

export function resolveImport(fromRel, spec, files) {
  const base = path.posix.normalize(path.posix.join(path.posix.dirname(fromRel), spec));
  for (const c of [base, `${base}.ts`, `${base}.tsx`, `${base}/index.ts`, `${base}/index.tsx`]) {
    if (files.has(c)) return c;
  }
  return null;
}

function stronglyConnected(graph) {
  let counter = 0;
  const index = new Map();
  const low = new Map();
  const onStack = new Set();
  const stack = [];
  const groups = [];
  const visit = (v) => {
    index.set(v, counter);
    low.set(v, counter++);
    stack.push(v);
    onStack.add(v);
    for (const w of graph.get(v) ?? []) {
      if (!index.has(w)) {
        visit(w);
        low.set(v, Math.min(low.get(v), low.get(w)));
      } else if (onStack.has(w)) low.set(v, Math.min(low.get(v), index.get(w)));
    }
    if (low.get(v) === index.get(v)) {
      const group = [];
      let w;
      do {
        w = stack.pop();
        onStack.delete(w);
        group.push(w);
      } while (w !== v);
      groups.push(group);
    }
  };
  for (const v of graph.keys()) if (!index.has(v)) visit(v);
  return groups;
}

/** sources: Map<relative path under src, text>. Returns violations and line counts. */
export function analyze(sources) {
  const files = new Set([...sources.keys()].filter((f) => SOURCE.test(f) && isHandWritten(f)));
  const direction = [];
  const deep = [];
  const unresolved = [];
  const graph = new Map([...files].map((f) => [f, new Set()]));
  for (const from of files) {
    for (const m of stripComments(sources.get(from)).matchAll(IMPORT)) {
      const to = resolveImport(from, m[1], files);
      if (!to) {
        const target = path.posix.normalize(path.posix.join(path.posix.dirname(from), m[1]));
        if (!ASSET.test(m[1]) && !target.startsWith("generated/")) unresolved.push(`${from}: ${m[1]}`);
        continue;
      }
      if (to === from) continue;
      graph.get(from).add(to);
      const a = layerOf(from);
      const b = layerOf(to);
      const up = b.layer === "app" || b.layer === "features";
      const apiUp = a.layer === "api" && (b.layer === "ui" || b.layer === "lib" || b.layer === "state");
      if ((SHARED.has(a.layer) && up) || (a.layer === "features" && b.layer === "app") || apiUp) direction.push(`${from} -> ${to}`);
      if (a.layer === "features" && b.layer === "features" && a.feature !== b.feature) {
        const front = /^features\/[^/]+\/index\.tsx?$/.test(to);
        if (!front) deep.push(`${from} -> ${to}`);
      }
    }
  }
  const cycle = [];
  for (const group of stronglyConnected(graph)) {
    if (group.length < 2) continue;
    const inside = new Set(group);
    for (const from of group) for (const to of graph.get(from)) if (inside.has(to)) cycle.push(`${from} -> ${to}`);
  }
  const featureGraph = new Map();
  for (const [from, tos] of graph) {
    const a = layerOf(from);
    if (a.layer !== "features") continue;
    if (!featureGraph.has(a.feature)) featureGraph.set(a.feature, new Set());
    for (const to of tos) {
      const b = layerOf(to);
      if (b.layer === "features" && b.feature !== a.feature) {
        featureGraph.get(a.feature).add(b.feature);
        if (!featureGraph.has(b.feature)) featureGraph.set(b.feature, new Set());
      }
    }
  }
  const featureCycle = [];
  for (const group of stronglyConnected(featureGraph)) {
    if (group.length < 2) continue;
    const inside = new Set(group);
    for (const from of group) for (const to of featureGraph.get(from)) if (inside.has(to)) featureCycle.push(`${from} -> ${to}`);
  }
  const lines = {};
  const css = {};
  for (const [rel, text] of sources) {
    if (!isHandWritten(rel)) continue;
    const n = text === "" ? 0 : text.split("\n").length - (text.endsWith("\n") ? 1 : 0);
    if (SOURCE.test(rel) && n > LIMITS.ts) lines[rel] = n;
    if (rel.endsWith(".css") && n > LIMITS.css) css[rel] = n;
  }
  const sort = (xs) => [...new Set(xs)].sort();
  const sortObj = (o) => Object.fromEntries(Object.entries(o).sort(([a], [b]) => (a < b ? -1 : 1)));
  return { direction: sort(direction), deep: sort(deep), cycle: sort(cycle), featureCycle: sort(featureCycle), lines: sortObj(lines), css: sortObj(css), unresolved: sort(unresolved) };
}

/** fresh: fails. stale: baselined items that are gone (fails). shrunk: still long but smaller (notice). */
export function compare(current, baseline) {
  const fresh = [];
  const stale = [];
  const shrunk = [];
  for (const rule of ["deep", "cycle", "featureCycle"]) for (const v of current[rule] ?? []) fresh.push(`${rule}: ${v}`);
  for (const rule of ["direction"]) {
    const known = new Set(baseline[rule] ?? []);
    const now = new Set(current[rule]);
    for (const v of now) if (!known.has(v)) fresh.push(`${rule}: ${v}`);
    for (const v of known) if (!now.has(v)) stale.push(`${rule}: ${v}`);
  }
  for (const [rule, limit] of [["lines", LIMITS.ts], ["css", LIMITS.css]]) {
    const known = baseline[rule] ?? {};
    for (const [file, n] of Object.entries(current[rule])) {
      if (!(file in known)) fresh.push(`${rule}: ${file} has ${n} lines, over the ${limit} limit`);
      else if (n > known[file]) fresh.push(`${rule}: ${file} grew from ${known[file]} to ${n} lines (limit ${limit})`);
      else if (n < known[file]) shrunk.push(`${rule}: ${file} shrank from ${known[file]} to ${n} lines`);
    }
    for (const file of Object.keys(known)) if (!(file in current[rule])) stale.push(`${rule}: ${file} is gone or back under ${limit} lines`);
  }
  return { fresh, stale, shrunk };
}

export function readSources(srcDir) {
  const out = new Map();
  const walk = (dir) => {
    for (const name of readdirSync(dir)) {
      const full = path.join(dir, name);
      if (statSync(full).isDirectory()) {
        if (!SKIP_DIRS.has(name) || dir !== srcDir) walk(full);
      } else if (/\.(ts|tsx|css)$/.test(name)) {
        out.set(path.relative(srcDir, full).split(path.sep).join("/"), readFileSync(full, "utf8"));
      }
    }
  };
  walk(srcDir);
  return out;
}

function main() {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
  const baselinePath = path.join(root, "scripts", "arch-baseline.json");
  const current = analyze(readSources(path.join(root, "src")));
  if (process.argv.includes("--update")) {
    const recorded = { ...current };
    delete recorded.unresolved;
    delete recorded.deep;
    delete recorded.cycle;
    delete recorded.featureCycle;
    const body = { _note: "Recorded architecture debt (docs/refactor.md). Only shrink it; npm run arch:update rewrites it.", limits: LIMITS, ...recorded };
    writeFileSync(baselinePath, `${JSON.stringify(body, null, 2)}\n`);
    console.log(`arch baseline written: ${current.direction.length} direction, ${Object.keys(current.lines).length} long ts, ${Object.keys(current.css).length} long css`);
    return;
  }
  const { fresh, stale, shrunk } = compare(current, JSON.parse(readFileSync(baselinePath, "utf8")));
  for (const u of current.unresolved) console.warn(`arch: warning: unresolved import ${u}`);
  if (shrunk.length) console.log(`arch: ${shrunk.length} file(s) shrank; run \`npm run arch:update\` to lower the recorded size.\n${shrunk.map((v) => `  ${v}`).join("\n")}`);
  if (fresh.length || stale.length) {
    if (fresh.length) console.error(`arch: ${fresh.length} new violation(s) (docs/refactor.md):\n${fresh.map((v) => `  ${v}`).join("\n")}`);
    if (stale.length) console.error(`arch: ${stale.length} baselined item(s) no longer exist; run \`npm run arch:update\` so the baseline shrinks:\n${stale.map((v) => `  ${v}`).join("\n")}`);
    process.exit(1);
  }
  console.log("arch: no new violations");
}

const norm = (href) => (process.platform === "win32" ? href.toLowerCase() : href);
if (process.argv[1] && norm(pathToFileURL(path.resolve(process.argv[1])).href) === norm(import.meta.url)) main();
