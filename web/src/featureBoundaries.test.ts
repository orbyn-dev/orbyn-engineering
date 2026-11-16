import { ESLint } from "eslint";
import { describe, expect, it } from "vitest";

const eslint = new ESLint();
async function lint(filePath: string, code: string) {
  const [result] = await eslint.lintText(code, { filePath });
  if (!result) throw new Error(`No lint result for ${filePath}`);
  expect(result.fatalErrorCount).toBe(0);
  return result.messages.filter((message) => message.ruleId === "architecture/boundaries");
}

describe.each([
  ["src/features/thread/probe.ts", "../usage"],
  ["src/app/probe.ts", "../features/usage"],
  ["src/features/usage/probe.ts", "../thread"],
  ["src/app/probe.ts", "../features/future"],
])("boundary from %s via %s", (filePath, target) => {
  it.each([
    `import { value } from "${target}/private";`,
    `import type { Value } from "${target}/private";`,
    `export { value } from "${target}/private";`,
    `export * from "${target}/private";`,
    `const value = import("${target}/private");`,
    `type Value = import("${target}/private").Value;`,
    `import { value } from "${target}/index/nested";`,
    `import { value } from "${target}/index.tsx";`,
  ])("rejects internals: %s", async (code) => {
    expect(await lint(filePath, code)).toMatchObject([{ messageId: "private" }]);
  });

  it.each([
    `import { value } from "${target}";`,
    `import { value } from "${target}/index";`,
    `import type { Value } from "${target}/index.ts";`,
    `export { value } from "${target}";`,
    `const value = import("${target}");`,
    `type Value = import("${target}").Value;`,
  ])("allows the front door: %s", async (code) => {
    expect(await lint(filePath, code)).toHaveLength(0);
  });
});

it("lets a feature use its own internals, and tests use anything", async () => {
  expect(await lint("src/features/thread/ThreadView.tsx", 'import { x } from "./composer/ComposerTraits";')).toHaveLength(0);
  expect(await lint("src/features/thread/ThreadView.tsx", 'import { x } from "../thread/composer/ComposerTraits";')).toHaveLength(0);
  expect(await lint("src/features/thread/a.test.ts", 'import { x } from "../usage/UsagePage";')).toHaveLength(0);
});

it("keeps shared folders from importing a feature", async () => {
  for (const file of ["src/ui/probe.ts", "src/lib/probe.ts", "src/api/probe.ts", "src/product/probe.ts", "src/state/probe.ts"]) {
    expect(await lint(file, 'import { x } from "../features/usage";')).toMatchObject([{ messageId: "inward" }]);
  }
});
