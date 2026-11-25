import { beforeEach, describe, expect, it } from "vitest";
import type { EngineModels, ModelInfo } from "../../api/types";
import type { MockApi } from "../../../mock/api";
import { installMockFetch } from "../../test-support";
import { holdFetch, wire } from "../../wire.test-support";
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

describe("the lists the service answers", () => {
  let mock: MockApi;
  /** A listing as a read that already landed left it. */
  const listed = (label: string, engine = "e"): EngineModels => ({
    engine, source: "engine", error: null, fallback: false, checked_at: "", models: [model(label)],
  });
  /** The model-list reads the page sent for one engine, in order. */
  const reads = (engine: string) => wire().filter((call) => call.path.startsWith(`/api/engines/${engine}/models`)).map((call) => call.path);

  beforeEach(() => {
    mock = installMockFetch();
    useModelStore.setState({ byEngine: {}, loading: {}, errors: {} });
  });

  it("reads a list once per engine, loading until it arrives", async () => {
    const engine = "load-once";
    mock.setEngineModels(engine, [model("only")]);
    const slow = holdFetch("GET", `/api/engines/${engine}/models`);
    const first = useModelStore.getState().load(engine);
    expect(useModelStore.getState().loading[engine]).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(reads(engine)).toEqual([`/api/engines/${engine}/models`]);

    slow.release();
    await first;
    expect(useModelStore.getState().loading[engine]).toBe(false);
    expect(useModelStore.getState().byEngine[engine]?.models.map((one) => one.id)).toEqual(["only"]);

    // The second open of the same engine shows the list it already holds and does not read again.
    await useModelStore.getState().load(engine);
    expect(reads(engine)).toHaveLength(1);
  });

  it("keeps a failed read's error and reads again on the next open", async () => {
    const engine = "failed-read";
    mock.fail(`/api/engines/${engine}/models`, 1, 502, "engine_unreachable", "The engine did not answer.");
    await useModelStore.getState().load(engine);
    expect(useModelStore.getState().errors[engine]).toBe("The engine did not answer.");
    expect(useModelStore.getState().loading[engine]).toBe(false);
    expect(useModelStore.getState().byEngine[engine]).toBeUndefined();

    await useModelStore.getState().load(engine);
    expect(useModelStore.getState().errors[engine]).toBeNull();
    expect(useModelStore.getState().byEngine[engine]?.models.length).toBeGreaterThan(0);
    expect(reads(engine)).toHaveLength(2);
  });

  it("asks for a fresh list on reload and shows what the service answers", async () => {
    const engine = "reload-forced";
    mock.setEngineModels(engine, [model("first")]);
    await useModelStore.getState().load(engine);
    mock.setEngineModels(engine, [model("second")]);

    await useModelStore.getState().reload(engine);

    expect(useModelStore.getState().byEngine[engine]?.models.map((one) => one.id)).toEqual(["second"]);
    expect(reads(engine)).toEqual([
      `/api/engines/${engine}/models`,
      `/api/engines/${engine}/models?refresh=true`,
    ]);
  });

  it("waits for a forced reload instead of making it lose its turn, so loading returns to false", async () => {
    const engine = "forced-wait";
    mock.setEngineModels(engine, [model("forced")]);
    useModelStore.setState({ byEngine: { [engine]: listed("before", engine) }, loading: {}, errors: {} });
    const slow = holdFetch("GET", `/api/engines/${engine}/models?refresh=true`);

    const forced = useModelStore.getState().reload(engine);
    expect(useModelStore.getState().loading[engine]).toBe(true);
    // A quiet read while the person's read runs would cut it short, so it does not start one.
    await useModelStore.getState().revalidate(engine);
    expect(reads(engine)).toEqual([`/api/engines/${engine}/models?refresh=true`]);

    slow.release();
    await forced;
    expect(useModelStore.getState().loading[engine]).toBe(false);
    expect(useModelStore.getState().byEngine[engine]?.models.map((one) => one.id)).toEqual(["forced"]);
  });

  it("drops an answer that a load made old while it was in flight", async () => {
    const engine = "old-answer";
    mock.setEngineModels(engine, [model("before")]);
    await useModelStore.getState().load(engine);
    const quiet = holdFetch("GET", `/api/engines/${engine}/models`);

    const revalidating = useModelStore.getState().revalidate(engine);
    await new Promise((resolve) => setTimeout(resolve, 0));
    mock.setEngineModels(engine, [model("forced")]);
    await useModelStore.getState().reload(engine);
    quiet.release();
    await revalidating;

    expect(useModelStore.getState().byEngine[engine]?.models.map((one) => one.id)).toEqual(["forced"]);
  });

  it("revalidates the lists on screen and shows the service's new models", async () => {
    const one = "revalidate-one";
    mock.setEngineModels(one, [model("before-one")]);
    await useModelStore.getState().load(one);
    mock.setEngineModels(one, [model("after-one")]);

    await useModelStore.getState().revalidate();

    expect(useModelStore.getState().byEngine[one]?.models.map((entry) => entry.id)).toEqual(["after-one"]);
    // The open read and the quiet read can use the same path; both were sent.
    expect(reads(one)).toHaveLength(2);
  });

  it("leaves a list alone when the service fails the quiet read, and reads nothing with no list on screen", async () => {
    const engine = "revalidate-failed";
    useModelStore.setState({ byEngine: { [engine]: listed("keep", engine) }, loading: {}, errors: {} });
    mock.fail(`/api/engines/${engine}/models`, 1, 500, "internal", "The service failed.");

    await useModelStore.getState().revalidate(engine);
    expect(useModelStore.getState().byEngine[engine]?.models.map((one) => one.id)).toEqual(["keep"]);
    expect(useModelStore.getState().errors[engine]).toBeUndefined();

    await useModelStore.getState().revalidate("never-loaded");
    expect(reads("never-loaded")).toEqual([]);
  });
});