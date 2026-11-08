import { defineConfig, devices } from "@playwright/test";

// Playwright's own headless shell from node_modules, never an installed browser.
process.env.PLAYWRIGHT_BROWSERS_PATH ??= "0";

/** Runs against the dev server's deterministic mock API; `e2e/real` starts the real service. */
export default defineConfig({
  testDir: "./e2e",
  // Needs the production build and half an hour; `playwright.perf.config.ts` runs it on purpose.
  testIgnore: /perf-baseline/,
  timeout: 30_000,
  // Twice the shell's 4 s refresh, so a row that arrives by polling never races the wait.
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  // WebKit is the engine of the macOS app's web view. It is opt-in (ORBYN_E2E_WEBKIT=1) and runs only the specs that name it,
  // because CI installs the Chromium shell alone.
  projects: [
    { name: "chromium" },
    ...(process.env.ORBYN_E2E_WEBKIT ? [{ name: "webkit", testMatch: /reading-layout\.spec\.ts/, use: { ...devices["Desktop Safari"] } }] : []),
  ],
  use: {
    baseURL: "http://localhost:5274",
    ...devices["Desktop Chrome"],
    headless: true,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    launchOptions: { args: ["--in-process-gpu"] },
  },
  webServer: {
    command: "npx vite --port 5274 --strictPort",
    url: "http://localhost:5274",
    reuseExistingServer: false,
  },
});
