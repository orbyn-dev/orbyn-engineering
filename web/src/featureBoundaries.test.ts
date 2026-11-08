import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";

const eslint = new ESLint();
async function lint(filePath: string, code: string, ruleId = "architecture/boundaries") {
  const [result] = await eslint.lintText(code, { filePath });
  if (!result) throw new Error(`No lint result for ${filePath}`);
  expect(result.fatalErrorCount).toBe(0);
  return result.messages.filter((message) => message.ruleId === ruleId);
}

describe.each([
  ["src/features/thread/probe.ts", "../usage"],
  ["src/app/probe.ts", "../features/usage"],
])("usage boundary from %s", (filePath, usage) => {
  it.each([
    `import { value } from "${usage}/private";`,
    `import type { Value } from "${usage}/private";`,
    `export { value } from "${usage}/private";`,
    `export * from "${usage}/private";`,
    `const value = import("${usage}/private");`,
    `type Value = import("${usage}/private").Value;`,
    `import { value } from "${usage}";`,
    `import { value } from "${usage}/public/nested";`,
    `import { value } from "${usage}/public.ts/nested";`,
    `import { value } from "${usage}/public.tsx";`,
    `import { value } from "${usage}/../usage/private";`,
  ])("rejects internals: %s", async (code) => {
    expect(await lint(filePath, code)).toMatchObject([{ messageId: "private" }]);
  });

  it.each([
    `import { value } from "${usage}/public";`,
    `import type { Value } from "${usage}/public.ts";`,
    `export { value } from "${usage}/public";`,
    `export * from "${usage}/public.ts";`,
    `const value = import("${usage}/public");`,
    `type Value = import("${usage}/public.ts").Value;`,
    `import { value } from "${usage}/../usage/public";`,
  ])("allows the public entry point: %s", async (code) => {
    expect(await lint(filePath, code)).toHaveLength(0);
  });
});

it("allows usage internals, inward imports and unfinished destinations", async () => {
  expect(await lint("src/features/usage/probe.ts", 'import { value } from "./private";')).toHaveLength(0);
  expect(await lint("src/features/thread/probe.ts", 'import { value } from "../../state/ui";')).toHaveLength(0);
  expect(await lint("src/features/thread/probe.ts", 'import { value } from "../panels/private";')).toHaveLength(0);
});

it.each(["ui", "state", "lib", "api", "styles"])("keeps %s inward, including public dynamic imports", async (folder) => {
  for (const code of [
    'import { value } from "../features/usage/public";',
    'export * from "../features/usage/public";',
    'const value = import("../features/usage/public");',
    'type Value = import("../features/usage/public").Value;',
  ]) {
    expect(await lint(`src/${folder}/probe.ts`, code)).toMatchObject([{ messageId: "inward" }]);
  }
});

it.each([
  ["src/features/thread/probe.test.ts", "../usage/private"],
  ["src/features/thread/probe.test.tsx", "../usage/private"],
  ["src/test-setup.ts", "./features/usage/private"],
  ["src/test-support.ts", "./features/usage/private"],
  ["mock/probe.ts", "../src/features/usage/private"],
  ["e2e/probe.ts", "../src/features/usage/private"],
])("preserves white-box fixture access in %s", async (filePath, target) => {
  expect(await lint(filePath, `import { value } from "${target}";`)).toHaveLength(0);
});

it.each([
  ["src/state/probe.ts", 'import { value } from "../features/usage/public";'],
  ["src/features/usage/probe.ts", 'import { value } from "@tauri-apps/api";'],
  ["src/features/usage/probe.ts", 'import { value } from "../../../mock/api";'],
])("preserves existing restrictions in %s: %s", async (filePath, code) => {
  expect(await lint(filePath, code, "no-restricted-imports")).toHaveLength(1);
});
