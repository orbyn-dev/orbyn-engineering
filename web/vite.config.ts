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
    chunkSizeWarningLimit: 700,
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
