import type { ClientRequest } from "node:http";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vitest/config";
import { mockApi } from "./mock/plugin.ts";

/**
 * `npm run dev` serves the deterministic mock API. Set ORBYN_API to a running
 * orbynd origin to proxy to the real service instead. The mock is a dev-server
 * plugin outside `src`, so no application module can import it into a build;
 * scripts/assert-no-mock.mjs checks the output of every production build.
 */
const realApi = process.env.ORBYN_API;

/**
 * Production chunk split, so a deploy re-downloads only what changed instead of a single app script:
 * the vendor chunks change with a dependency upgrade, `app-core` holds the shared layers, and the UI
 * is split along feature boundaries. Chunk names follow web/src/features.
 *
 * Vite 8's rolldown bundler deprecates rollupOptions/manualChunks in favour of output.codeSplitting.
 * Groups are ordered the way features depend on each other: a group's imports already sit in a group
 * above it, and a chunk does not pull its dependencies in (includeDependenciesRecursively is off), so
 * every chunk imports only chunks ordered before it. That keeps the chunk graph acyclic, and each
 * chunk's top-level code executes after what it imports.
 */
const chunkGroups = [
  // Vite injects a preload helper into the chunk that hosts it; give it its own chunk so no other
  // chunk has to import the entry chunk back to reach it.
  { name: "vendor-runtime", test: /preload-helper/ },
  { name: "vendor-react", test: /node_modules[\\/](?:react|react-dom|scheduler)[\\/]/ },
  // `$initial` keeps lucide's thousands of dynamic icon modules out: only the icons the app imports
  // statically, and the name-to-loader map the project icon picker searches, join this chunk.
  { name: "vendor-icons", test: /node_modules[\\/]lucide-react[\\/]/, tags: ["$initial" as const] },
  // The shared layers, and the only other static dependency shared between them (zustand).
  {
    name: "app-core",
    test: (id: string) =>
      /[\\/]src[\\/](?:api|generated|lib|product|styles|ui)[\\/]/.test(id) || /node_modules[\\/]zustand[\\/]/.test(id),
  },
  // Features the rest build on: none of these imports another feature.
  { name: "app-foundation", test: /[\\/]src[\\/]features[\\/](?:account|codemode|content|cursorthread|devices|enginehost|library|platform|profiles|snapshot|usage)[\\/]/ },
  { name: "app-crew", test: /[\\/]src[\\/]features[\\/]crew[\\/]/ },
  // Source-control and workspace pages, built on the chunks above.
  { name: "app-pages", test: /[\\/]src[\\/]features[\\/](?:family|issues|pulls|repos|skills|tools|workspace)[\\/]/ },
  { name: "app-panels", test: /[\\/]src[\\/]features[\\/]panels[\\/]/ },
  { name: "app-thread", test: /[\\/]src[\\/]features[\\/]thread[\\/]/ },
  // Settings reaches every feature; onboarding reaches thread and workspace. They stay apart so the
  // home screen, which is an onboarding screen, does not fetch Settings with it.
  { name: "app-onboarding", test: /[\\/]src[\\/]features[\\/]onboarding[\\/]/ },
  { name: "app-settings", test: /[\\/]src[\\/]features[\\/]settings[\\/]/ },
  // A feature not named above waits for every group it might import; a feature that others import
  // instead belongs in the group before its dependents.
  { name: "app-features", test: /[\\/]src[\\/]features[\\/]/ },
];

/** The service accepts only its own Origin, and the browser's here is the dev port. */
const dropOrigin = (proxy: { on(event: "proxyReq", listener: (request: ClientRequest) => void): unknown }) =>
  proxy.on("proxyReq", (request) => request.removeHeader("origin"));

/**
 * The service serves this build with `font-src 'self'` (crates/orbynd/src/routes/ui.rs), so a font
 * inlined as a `data:` URI is blocked and the text falls back to a system font. Vite's default
 * inlines assets under 4 KiB, which caught the small KaTeX and JetBrains Mono subsets, so every
 * font stays a file under /assets/ and every other asset keeps the default rule.
 * scripts/assert-no-inlined-fonts.mjs checks the output of every production build.
 */
const fontAsset = /\.(?:woff2?|ttf|otf|eot)$/i;

export default defineConfig(({ mode }) => ({
  // The platform design screens (src/features/platform) exist only outside production builds.
  define: { __ORBYN_PREVIEWS__: JSON.stringify(mode !== "production") },
  plugins: [react(), ...(realApi ? [] : [mockApi()])],
  server: {
    port: 5273,
    strictPort: false,
    proxy: realApi
      ? {
          "/api": { target: realApi, configure: dropOrigin },
          "/auth": { target: realApi, configure: dropOrigin },
          "/ws": { target: realApi, ws: true },
        }
      : undefined,
  },
  build: {
    target: "es2022",
    sourcemap: false,
    // A chunk that outgrows the default 500 kB warning is a split to revisit, not to hide.
    rolldownOptions: {
      output: { codeSplitting: { includeDependenciesRecursively: false, groups: chunkGroups } },
    },
    assetsInlineLimit: (filePath) => (fontAsset.test(filePath) ? false : undefined),
  },
  test: {
    environment: "jsdom",
    globals: true,
    setupFiles: ["./src/test-setup.ts"],
    include: ["src/**/*.test.{ts,tsx}", "mock/**/*.test.ts", "scripts/**/*.test.mjs", "tools/**/*.test.ts"],
    css: false,
  },
}));
