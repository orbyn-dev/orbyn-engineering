import { describe, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import type { ModelInfo } from "../../api/types";
import { useModelStore, arrangeModels, effortValue, resolveModel, searchRows, shortModelName } from "./modelStore";

const model = (id: string, over: Partial<ModelInfo> = {}): ModelInfo => ({
  id,
  label: id,
  description: null,
  is_default: false,
  custom: false,
  options: [],
  ...over,
});

const effort = {
  id: "effort",
  label: "Reasoning",
  values: [
    { id: "low", label: "Low", default: false },
    { id: "medium", label: "Medium", default: true },
  ],
};

describe("model lists", () => {
  it("drops hidden models and honours the saved order", () => {
    const list = [model("a"), model("b"), model("c")];
    const arranged = arrangeModels("e", list, ["e:b"], ["e:c", "e:a"]);
    expect(arranged.map((entry) => entry.id)).toEqual(["c", "a"]);
  });

  it("falls back to the engine's default model and effort", () => {
    const listed = { engine: "e", source: "engine", error: null, fallback: false, checked_at: "", models: [model("a"), model("b", { is_default: true, options: [effort] })] };
    const picked = resolveModel(listed, null);
    expect(picked?.id).toBe("b");
    expect(effortValue(picked, null)?.id).toBe("medium");
    expect(effortValue(picked, "low")?.label).toBe("Low");
    expect(effortValue(model("x"), "low")).toBeNull();
  });

  it("matches every word and shortens vendor prefixes", () => {
    const rows = [
      { engine: "codex", model: model("gpt-6-luna", { label: "GPT-6-Luna" }) },
      { engine: "claude", model: model("opus", { label: "Claude Opus" }) },
    ];
    expect(searchRows(rows, "6 luna").map((row) => row.engine)).toEqual(["codex"]);
    expect(shortModelName("GPT-6-Luna")).toBe("6-Luna");
    expect(shortModelName("Claude Opus 5.5")).toBe("Opus 5.5");
  });
});

describe("revalidating the model lists", () => {
  const listed = (label: string) => ({ engine: "e1", source: "engine", error: null, fallback: false, checked_at: "", models: [model(label)] });

  it("waits for a forced reload instead of making it lose its turn, so loading returns to false", async () => {
    let finishForced!: (value: ReturnType<typeof listed>) => void;
    const spy = vi.spyOn(api, "models").mockImplementation(((_e: string, _s?: AbortSignal, refresh?: boolean) =>
      refresh ? new Promise((resolve) => (finishForced = resolve as never)) : Promise.resolve(listed("stale"))) as never);
    useModelStore.setState({ byEngine: { e1: listed("before") as never }, loading: {}, errors: {} });
    const forced = useModelStore.getState().reload("e1");
    expect(useModelStore.getState().loading.e1).toBe(true);
    await useModelStore.getState().revalidate("e1");
    finishForced(listed("forced"));
    await forced;
    expect(useModelStore.getState().loading.e1).toBe(false);
    expect(useModelStore.getState().byEngine.e1?.models[0]?.id).toBe("forced");
    spy.mockRestore();
  });

  it("drops an answer that a load made old while it was in flight", async () => {
    let finishQuiet!: (value: ReturnType<typeof listed>) => void;
    const spy = vi.spyOn(api, "models").mockImplementation(((_e: string, _s?: AbortSignal, refresh?: boolean) =>
      refresh ? Promise.resolve(listed("forced")) : new Promise((resolve) => (finishQuiet = resolve as never))) as never);
    useModelStore.setState({ byEngine: { e1: listed("before") as never }, loading: {}, errors: {} });
    const quiet = useModelStore.getState().revalidate("e1");
    await useModelStore.getState().reload("e1");
    finishQuiet(listed("stale"));
    await quiet;
    expect(useModelStore.getState().byEngine.e1?.models[0]?.id).toBe("forced");
    spy.mockRestore();
  });
});
