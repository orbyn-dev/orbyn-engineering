import { expect, it } from "vitest";
import { exampleTierView } from "../../../../../mock/reviewControls";
import { copyTiers, tierSave } from "./engineTierDraft";

it("preserves the whole map and exact saved pins rather than repinning untouched rows", () => {
  const view = exampleTierView(); view.engines.push({ ...view.engines[2]!, id: "other-acp", name: "Other", defaults: ["other-default"] });
  view.settings.assignments["other-acp"] = { tier: "scout", model: "other-default" };
  const draft = copyTiers(view.settings.assignments); draft["example-acp"]!.tier = "lead";
  const body = tierSave(view.settings, draft, view);
  expect(body.seen).toBe(1);
  expect(body.assignments).toContainEqual({ engine: "other-acp", tier: "scout", expected_default_model: "other-default" });
  view.engines[3]!.defaults = ["changed-default"];
  expect(() => tierSave(view.settings, draft, view)).toThrow("default model changed");
  expect(draft["other-acp"]!.model).toBe("other-default");
});
it("refuses absent/ambiguous defaults, unknown or duplicate engines and never infers tiers", () => {
  for (const [defaults, message] of [[[], /no available default model/], [["one", "two"], /more than one default model/]] as const) {
    const view = exampleTierView(); view.engines[2]!.defaults = [...defaults];
    expect(() => tierSave(view.settings, copyTiers(view.settings.assignments), view)).toThrow(message);
  }
  const view = exampleTierView(), draft = copyTiers(view.settings.assignments);
  draft["unknown"] = { tier: "lead", model: "sounds-powerful" };
  expect(() => tierSave(view.settings, draft, view)).toThrow("unavailable");
  delete draft["unknown"]; view.engines.push(view.engines[2]!);
  expect(() => tierSave(view.settings, draft, view)).toThrow("Refresh engines");
});
