// DEVELOPMENT ONLY. Draws screens of the dev-only mock app with Playwright: one dev server on a free port (`vite`, which serves
// web/mock), one browser, each frame in each theme at 1400x760. Nothing here reaches a real service, an account or the network.
import { spawn } from "node:child_process";
import { createServer } from "node:net";
import { mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { createRequire } from "node:module";

export const THEMES = ["light", "dark", "dim"];
const VIEWPORT = { width: 1400, height: 760 };
/** No single screen may hold the run: a shot that takes longer than this is given up on and reported. */
const SHOT_MS = 60_000;
const within = (work, ms, what) =>
  Promise.race([work, new Promise((_, reject) => setTimeout(() => reject(new Error(`${what} took longer than ${ms / 1000} s`)), ms))]);

const freePort = () =>
  new Promise((resolve, reject) => {
    const probe = createServer();
    probe.on("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = /** @type {import("node:net").AddressInfo} */ (probe.address());
      probe.close(() => resolve(port));
    });
  });

/** Starts `vite` (the mock API is part of its dev server) and waits until it answers. Returns the origin and a stop function. */
export async function startMockApp(web) {
  const port = await freePort();
  const child = spawn(process.execPath, [join(web, "node_modules/vite/bin/vite.js"), "--port", String(port), "--strictPort", "--host", "127.0.0.1"], {
    cwd: web,
    stdio: ["ignore", "pipe", "pipe"],
    env: { ...process.env, ORBYN_API: "" },
  });
  let log = "";
  child.stdout.on("data", (chunk) => (log += chunk));
  child.stderr.on("data", (chunk) => (log += chunk));
  const origin = `http://127.0.0.1:${port}`;
  const stop = () => child.kill("SIGTERM");
  for (let attempt = 0; attempt < 120; attempt += 1) {
    if (child.exitCode !== null) throw new Error(`the dev server exited: ${log.slice(-400)}`);
    try {
      if ((await fetch(`${origin}/api/info`)).status < 500) return { origin, stop };
    } catch {
      /* not up yet */
    }
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  stop();
  throw new Error(`the dev server did not start: ${log.slice(-400)}`);
}

const escape = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
const nameOf = (step) => (step.exact ? new RegExp(`^${escape(step.name)}$`) : step.prefix ? new RegExp(`^${escape(step.name)}`) : step.name);

async function route(page, spec) {
  await page.route(spec.route, async (request) => {
    if (spec.delay) await new Promise((resolve) => setTimeout(resolve, spec.delay));
    if (spec.status) return request.fulfill({ status: spec.status, contentType: "application/json", body: JSON.stringify({ code: "bad_gateway", message: "The forge did not answer." }) });
    return request.continue();
  });
}

async function run(page, step) {
  switch (step.do) {
    case "click":
      await page.getByRole(step.role, { name: nameOf(step), exact: step.exact }).first().click();
      await page.waitForTimeout(400);
      return;
    case "fill":
      await page.getByRole("textbox", { name: step.label }).fill(step.value);
      return;
    case "press":
      await page.keyboard.press(step.key);
      await page.waitForTimeout(300);
      return;
    case "route":
      await route(page, step);
      return;
    case "offline":
      await page.context().setOffline(true);
      await page.waitForTimeout(300);
      return;
    default:
      throw new Error(`unknown step ${step.do}`);
  }
}

/** One frame in one theme: a fresh page, the frame's network set-up, its route, its steps, a screenshot. */
async function draw({ browser, app, id, frame, theme, out }) {
  const context = await browser.newContext({ viewport: { ...VIEWPORT, width: frame.width ?? VIEWPORT.width }, reducedMotion: "reduce" });
  try {
    const page = await context.newPage();
    page.setDefaultTimeout(15_000);
    const prefs = { welcomeDone: true, foldersNotice: false, theme: theme === "light" ? "light" : "dark", darkPreset: theme === "dim" ? "dim" : null };
    await page.addInitScript((value) => {
      try {
        if (!sessionStorage.getItem("__seeded")) {
          localStorage.setItem("orbyn.prefs.v1", JSON.stringify(value));
          sessionStorage.setItem("__seeded", "1");
        }
      } catch {
        /* a window with no storage still draws */
      }
    }, prefs);
    for (const spec of frame.pre ?? []) await route(page, spec);
    const to = encodeURIComponent(`/${frame.route}`).replace("%2F", "/");
    await page.goto(`${app.origin}/__mock/scenario/${frame.scenario ?? "default"}?to=${to}`);
    await page.waitForSelector(".sidebar, .main", { timeout: 20000 }).catch(() => {});
    await page.waitForTimeout(frame.wait ?? 1500);
    await page.evaluate(() => document.querySelectorAll(".folders-notice, .toast").forEach((node) => node.remove()));
    for (const step of frame.steps ?? []) await run(page, step);
    await page.waitForTimeout(500);
    const file = join(out, `${id}-${theme}.png`);
    mkdirSync(dirname(file), { recursive: true });
    await page.screenshot({ path: file });
    return file;
  } finally {
    await Promise.race([context.close(), new Promise((resolve) => setTimeout(resolve, 10_000))]);
  }
}

/**
 * @param {object} p
 * @param {string} p.web the web folder
 * @param {string[]} p.ids frames to draw
 * @param {Record<string, any>} p.catalog
 * @param {string} p.out folder for `<id>-<theme>.png`
 * @param {(line: string) => void} [p.log]
 * @returns {Promise<{ files: Record<string, Record<string, string>>, failed: string[] }>}
 */
export async function shoot({ web, ids, catalog, out, log = () => {} }) {
  const require = createRequire(join(web, "package.json"));
  const { chromium } = require("playwright-core");
  mkdirSync(out, { recursive: true });
  const app = await startMockApp(web);
  const browser = await chromium.launch();
  const files = {};
  const failed = [];
  try {
    for (const id of ids) {
      for (const theme of THEMES) {
        try {
          (files[id] ??= {})[theme] = await within(draw({ browser, app, id, frame: catalog[id], theme, out }), SHOT_MS, `${id} (${theme})`);
        } catch (error) {
          log(`${id} (${theme}): ${String(error).split("\n")[0]}`);
        }
      }
      if (!files[id] || Object.keys(files[id]).length === 0) failed.push(id);
    }
  } finally {
    // A browser that will not close must not hold the run: it is given a few seconds and then left to the end of the process.
    await Promise.race([browser.close().catch(() => {}), new Promise((resolve) => setTimeout(resolve, 10_000))]);
    app.stop();
  }
  return { files, failed };
}
