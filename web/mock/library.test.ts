import { expect, it } from "vitest";
import { createLibraryMock } from "./library.ts";

const EXPORT = { format: "orbyn-library", version: 1, exported_at: "2026-11-22T00:00:00Z", profiles: [], workflows: [], prompts: [], env_sets: [] };

it("refuses an entry whose params are not the contract's Param[], instead of storing one a later render throws on", async () => {
  const malformed: unknown[] = [
    [null],
    [{ name: "port", kind: "text", required: "yes" }],
    [{ kind: "text", label: "Port", required: true }],
    [{ name: "port", kind: "number", required: true }],
  ];
  for (const params of malformed) {
    const mock = createLibraryMock();
    const answer = await mock.importLibrary({
      ...EXPORT,
      workflows: [{ name: "Deploy", description: "", command: "deploy", params, env_set_names: [] }],
      prompts: [{ name: "Ask", description: "", body: "ask", params }],
    }, false);
    expect(answer).toMatchObject({ dry_run: false, created: 0, renamed: 0, refused: 2 });
    expect(answer.lines).toEqual([
      { kind: "workflow", index: 0, outcome: "refused", code: "invalid_input" },
      { kind: "prompt", index: 0, outcome: "refused", code: "invalid_input" },
    ]);
    expect(await mock.workflows()).toHaveLength(2);
    expect(await mock.prompts()).toHaveLength(1);
  }
});

it("stores the parameters of a valid imported entry and renders them", async () => {
  const mock = createLibraryMock();
  const params = [{ name: "port", label: "Port", kind: "text", default: "5173", required: true }];
  const answer = await mock.importLibrary({
    ...EXPORT,
    workflows: [{ name: "Deploy", description: "", command: "deploy {{port}}", params, env_set_names: [] }],
    prompts: [{ name: "Ask", description: "", body: "ask about {{port}}", params }],
  }, false);
  expect(answer.lines).toEqual([
    { kind: "workflow", index: 0, outcome: "created", name: "Deploy" },
    { kind: "prompt", index: 0, outcome: "created", name: "Ask" },
  ]);
  const workflow = (await mock.workflows()).find((one) => one.name === "Deploy");
  expect(workflow!.params).toEqual(params);
  expect(await mock.renderWorkflow(workflow!.id, {})).toEqual({ command: "deploy '5173'" });
  const prompt = (await mock.prompts()).find((one) => one.name === "Ask");
  expect(prompt!.params).toEqual(params);
  expect(await mock.renderPrompt(prompt!.id, { port: "8080" })).toEqual({ text: "ask about '8080'" });
});
