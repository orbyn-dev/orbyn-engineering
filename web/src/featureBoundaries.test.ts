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
  ["src/features/thread/probe.ts", "../usage", "usage"],
  ["src/app/probe.ts", "../features/usage", "usage"],
  ["src/features/thread/probe.ts", "../platform", "platform"],
  ["src/app/probe.ts", "../features/platform", "platform"],
  ["src/features/thread/probe.ts", "../settings", "settings"],
  ["src/app/probe.ts", "../features/settings", "settings"],
  ["src/features/workspace/probe.ts", "../skills", "skills"],
  ["src/app/probe.ts", "../features/skills", "skills"],
  ["src/features/thread/probe.ts", "../pulls", "pulls"],
  ["src/app/probe.ts", "../features/pulls", "pulls"],
])("boundary from %s via %s", (filePath, target, feature) => {
  it.each([
    `import { value } from "${target}/private";`,
    `import type { Value } from "${target}/private";`,
    `export { value } from "${target}/private";`,
    `export * from "${target}/private";`,
    `const value = import("${target}/private");`,
    `type Value = import("${target}/private").Value;`,
    `import { value } from "${target}";`,
    `import { value } from "${target}/public/nested";`,
    `import { value } from "${target}/public.ts/nested";`,
    `import { value } from "${target}/public.tsx";`,
    `import { value } from "${target}/../${feature}/private";`,
  ])("rejects internals: %s", async (code) => {
    expect(await lint(filePath, code)).toMatchObject([{ messageId: "private" }]);
  });

  it.each([
    `import { value } from "${target}/public";`,
    `import type { Value } from "${target}/public.ts";`,
    `export { value } from "${target}/public";`,
    `export * from "${target}/public.ts";`,
    `const value = import("${target}/public");`,
    `type Value = import("${target}/public.ts").Value;`,
    `import { value } from "${target}/../${feature}/public";`,
  ])("allows the public entry point: %s", async (code) => {
    expect(await lint(filePath, code)).toHaveLength(0);
  });
});

it("enforces settings boundaries on existing module paths", async () => {
  expect(await lint("src/app/Shell.tsx", 'import { SettingsPage } from "../features/settings/SettingsPage";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/features/thread/ThreadView.tsx", 'export * from "../settings/ScheduledTasks";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/app/TitleBar.tsx", 'import { labels } from "../features/settings/public";')).toHaveLength(0);
  expect(await lint("src/features/settings/SettingsPage.tsx", 'import { ScheduledTasksSection } from "./ScheduledTasks";')).toHaveLength(0);
  expect(await lint("src/state/ui.ts", 'const settings = import("../features/settings/public");')).toMatchObject([{ messageId: "inward" }]);
  expect(await lint("src/features/thread/settings-probe.test.ts", 'import { SettingsPage } from "../settings/SettingsPage";')).toHaveLength(0);
});

it("enforces skills boundaries on existing module paths", async () => {
  expect(await lint("src/app/Shell.tsx", 'import { SkillsPage } from "../features/skills/SkillsPage";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/features/workspace/Sidebar.tsx", 'export * from "../skills/SkillsNav";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/features/workspace/Sidebar.tsx", 'import { SkillsNav } from "../skills/public";')).toHaveLength(0);
  expect(await lint("src/features/skills/SkillsNav.tsx", 'import { useSkills } from "./skillsStore";')).toHaveLength(0);
  expect(await lint("src/state/ui.ts", 'const skills = import("../features/skills/public");')).toMatchObject([{ messageId: "inward" }]);
  expect(await lint("src/features/skills/SkillsPage.test.tsx", 'import { categoriesOf } from "./SkillsPage";')).toHaveLength(0);
});

it("enforces pulls boundaries on existing module paths", async () => {
  expect(await lint("src/app/Shell.tsx", 'import { PullsPage } from "../features/pulls/PullsPage";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/features/thread/PullChip.tsx", 'import "../pulls/pullsToolbar.css";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/features/thread/MarkdownLink.tsx", 'export * from "../pulls/pullLink";')).toMatchObject([{ messageId: "private" }]);
  expect(await lint("src/features/thread/pullStore.ts", 'import { checksOf } from "../pulls/public";')).toHaveLength(0);
  expect(await lint("src/features/pulls/PullsToolbar.tsx", 'import "./pullsToolbar.css";')).toHaveLength(0);
  expect(await lint("src/state/ui.ts", 'const pulls = import("../features/pulls/public");')).toMatchObject([{ messageId: "inward" }]);
  expect(await lint("src/features/pulls/pullModel.test.ts", 'import { checksOf } from "./pullModel";')).toHaveLength(0);
});

it.each(["usage", "platform", "settings", "skills", "pulls"])("allows %s internals, inward imports and unfinished destinations", async (feature) => {
  expect(await lint(`src/features/${feature}/probe.ts`, 'import { value } from "./private";')).toHaveLength(0);
  expect(await lint("src/features/thread/probe.ts", 'import { value } from "../../state/ui";')).toHaveLength(0);
  expect(await lint("src/features/thread/probe.ts", 'import { value } from "../panels/private";')).toHaveLength(0);
});

it.each(["usage", "platform", "settings", "skills", "pulls"])("keeps shared folders inward to %s, including public dynamic imports", async (feature) => {
  for (const folder of ["ui", "state", "lib", "api", "styles"]) {
    for (const code of [
      `import { value } from "../features/${feature}/public";`,
      `export * from "../features/${feature}/public";`,
      `const value = import("../features/${feature}/public");`,
      `type Value = import("../features/${feature}/public").Value;`,
    ]) {
      expect(await lint(`src/${folder}/probe.ts`, code)).toMatchObject([{ messageId: "inward" }]);
    }
  }
});

it.each([
  ["src/features/thread/probe.test.ts", "../usage/private"],
  ["src/features/thread/probe.test.tsx", "../usage/private"],
  ["src/features/thread/platform-probe.test.ts", "../platform/private"],
  ["src/features/thread/settings-probe.test.ts", "../settings/SettingsPage"],
  ["src/features/workspace/skills-probe.test.ts", "../skills/SkillsNav"],
  ["src/features/thread/pulls-probe.test.ts", "../pulls/pullModel"],
  ["src/test-setup.ts", "./features/pulls/PullsPage"],
  ["src/test-support.ts", "./features/pulls/pullLink"],
  ["src/test-setup.ts", "./features/skills/skillsStore"],
  ["src/test-support.ts", "./features/skills/SkillsPage"],
  ["src/test-setup.ts", "./features/settings/ScheduledTasks"],
  ["src/test-support.ts", "./features/settings/SettingsPage"],
  ["src/test-setup.ts", "./features/usage/private"],
  ["src/test-support.ts", "./features/usage/private"],
  ["mock/probe.ts", "../src/features/usage/private"],
  ["e2e/probe.ts", "../src/features/usage/private"],
])("preserves white-box fixture access in %s", async (filePath, target) => {
  expect(await lint(filePath, `import { value } from "${target}";`)).toHaveLength(0);
});

it.each([
  ["src/state/probe.ts", 'import { value } from "../features/usage/public";'],
  ["src/state/probe.ts", 'import { value } from "../features/platform/public";'],
  ["src/state/probe.ts", 'import { value } from "../features/settings/public";'],
  ["src/state/probe.ts", 'import { value } from "../features/skills/public";'],
  ["src/state/probe.ts", 'import { value } from "../features/pulls/public";'],
  ["src/features/usage/probe.ts", 'import { value } from "@tauri-apps/api";'],
  ["src/features/usage/probe.ts", 'import { value } from "../../../mock/api";'],
])("preserves existing restrictions in %s: %s", async (filePath, code) => {
  expect(await lint(filePath, code, "no-restricted-imports")).toHaveLength(1);
});
