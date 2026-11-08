import { defineConfig, devices } from "@playwright/test";
process.env.PLAYWRIGHT_BROWSERS_PATH ??= "0";
export default defineConfig({ testDir: "./e2e/real", testMatch: /perf-baseline/, timeout: 1_800_000, workers: 1, expect: { timeout: 15_000 }, use: { ...devices["Desktop Chrome"], actionTimeout: 15_000, headless: true, launchOptions: { args: ["--in-process-gpu"] } } });
