#!/usr/bin/env node
// Architecture guardrails for web/src (docs/refactor.md). Zero dependencies.
//
//   node scripts/arch-check.mjs            fail on NEW violations only
//   node scripts/arch-check.mjs --update   rewrite scripts/arch-baseline.json
//
// Rules, over hand-written files (tests, generated code and test helpers excluded):
//   direction  ui, lib, state and api never import app or a feature
//   deep       a feature imports another feature only through its index.ts
//   cycle      no import cycle between files; an edge inside a cycle is a violation
//   lines      TypeScript over 600 lines, CSS over 1500 lines; may not grow past baseline
// Existing violations live in the baseline; fixing one only needs a re-run of --update.
import { readFileSync, readdirSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

export const LIMITS = { ts: 600, css: 1500 };
const SHARED = new Set(["ui", "lib", "state", "api"]);
const SKIP_DIRS = new Set(["generated", "node_modules"]);
const TEST_FILE = /\.test\.(ts|tsx)$|(^|\/)test-(setup|support)\.ts$/;
const SOURCE = /\.(ts|tsx)$/;
const IMPORT = /(?:\bfrom\s*|\bimport\s*\(?\s*)["'](\.{1,2}\/[^"']*|\.{1,2})["']/g;

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
  const graph = new Map([...files].map((f) => [f, new Set()]));
  for (const from of files) {
    for (const m of sources.get(from).matchAll(IMPORT)) {
      const to = resolveImport(from, m[1], files);
      if (!to || to === from) continue;
      graph.get(from).add(to);
      const a = layerOf(from);
      const b = layerOf(to);
      if (SHARED.has(a.layer) && (b.layer === "app" || b.layer === "features")) direction.push(`${from} -> ${to}`);
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
  return { direction: sort(direction), deep: sort(deep), cycle: sort(cycle), lines: sortObj(lines), css: sortObj(css) };
}

/** What the baseline does not already allow. */
export function compare(current, baseline) {
  const fresh = [];
  const stale = [];
  for (const rule of ["direction", "deep", "cycle"]) {
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
    }
    for (const file of Object.keys(known)) if (!(file in current[rule])) stale.push(`${rule}: ${file}`);
  }
  return { fresh, stale };
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
    const body = { _note: "Recorded architecture debt (docs/refactor.md). Only shrink it; npm run arch:update rewrites it.", limits: LIMITS, ...current };
    writeFileSync(baselinePath, `${JSON.stringify(body, null, 2)}\n`);
    console.log(`arch baseline written: ${current.direction.length} direction, ${current.deep.length} deep, ${current.cycle.length} cycle, ${Object.keys(current.lines).length} long ts, ${Object.keys(current.css).length} long css`);
    return;
  }
  const { fresh, stale } = compare(current, JSON.parse(readFileSync(baselinePath, "utf8")));
  if (stale.length) console.log(`arch: ${stale.length} baselined item(s) are fixed; run \`npm run arch:update\` to shrink the baseline.`);
  if (fresh.length) {
    console.error(`arch: ${fresh.length} new violation(s) (docs/refactor.md):\n${fresh.map((v) => `  ${v}`).join("\n")}`);
    process.exit(1);
  }
  console.log("arch: no new violations");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
