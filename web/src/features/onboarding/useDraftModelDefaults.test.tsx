import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../mock/api";
import type { EngineStatus } from "../../api/types";
import { installMockFetch } from "../../test-support";
import { wire } from "../../wire.test-support";
import { holdEach } from "../../wire.held.test-support";
import { useDraftModelDefaults } from "./useDraftModelDefaults";
import { useUi } from "../../product/state/ui";

const engines = ["codex", "other"].map((id) => ({ id, installed: true, auth: "logged_in", capabilities: { managed: true } })) as EngineStatus[];
const remembered = { engine: "codex", model: "remembered", effort: "high" };
let mock: MockApi;
/** The saved defaults of one of the mock service's projects ("p-app", "p-site"), as Settings writes them. */
const save = (projectId: string, engine = "", model = "", rest: Record<string, unknown> = {}) =>
  mock.handle("PUT", `/api/projects/${projectId}/preferences`, { values: { default_engine: engine, default_model: model, disabled_engines: [], ...rest } });
const reads = () => wire().filter((call) => call.path.startsWith("/api/preferences"));
beforeEach(() => { mock = installMockFetch(); });

it("uses the effective checkout's model before the remembered model without carrying foreign effort", async () => {
  save("p-app", "other", "project-model");
  const { result } = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  expect(result.current.loading).toBe(true);
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.engine?.id).toBe("other");
  expect(result.current.model).toBe("project-model"); expect(result.current.effort).toBeNull();
  expect(reads()[0]?.path).toBe("/api/preferences?project_id=p-app");
});

it("keeps Automatic on the existing remembered choice and retains effort only with that model", async () => {
  const { result } = renderHook(() => useDraftModelDefaults(null, engines, remembered));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.engine?.id).toBe("codex");
  expect(result.current.model).toBe("remembered"); expect(result.current.effort).toBe("high");
  expect(reads()[0]?.path).toBe("/api/preferences");
});

it("does not let a late default replace a person's explicit choice", async () => {
  save("p-app", "other", "late");
  const held = holdEach("GET", "/api/preferences?project_id=p-app");
  const { result } = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  act(() => result.current.choose({ engine: "codex", model: "picked", effort: "low" }));
  await waitFor(() => expect(held).toHaveLength(1));
  await act(async () => held[0]!.release());
  expect(result.current.model).toBe("picked"); expect(result.current.engine?.id).toBe("codex");
  expect(result.current.effort).toBe("low");
});

it("fences A-to-B-to-A responses and does not carry another project's explicit choice", async () => {
  const held = holdEach("GET", /^\/api\/preferences\?project_id=/);
  save("p-app", "other", "old-a"); save("p-site", "other", "old-b");
  const { result, rerender } = renderHook(({ id }) => useDraftModelDefaults(id, engines, remembered), { initialProps: { id: "p-app" } });
  act(() => result.current.choose({ engine: "codex", model: "picked-a", effort: null }));
  rerender({ id: "p-site" });
  expect(result.current.explicit).toBe(false); expect(result.current.loading).toBe(true);
  save("p-app", "other", "new-a");
  rerender({ id: "p-app" });
  await waitFor(() => expect(held).toHaveLength(3));
  await act(async () => { held[0]!.release(); held[1]!.release(); });
  expect(result.current.loading).toBe(true);
  await act(async () => held[2]!.release());
  expect(result.current.loading).toBe(false);
  expect(result.current.model).toBe("picked-a");
});

it("does not silently substitute another provider when a configured default is disabled or missing", async () => {
  mock.handle("PUT", "/api/projects/p-app/preferences", { values: { default_engine: "other", default_model: "m", disabled_engines: ["other"] } });
  const { result } = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.engine).toBeNull(); expect(result.current.error).toMatch(/unavailable/);
  expect(result.current.installed.map((engine) => engine.id)).toEqual(["codex"]);
  act(() => result.current.choose({ engine: "codex", model: "picked", effort: null }));
  expect(result.current.engine?.id).toBe("codex"); expect(result.current.error).toBeNull();
});

it("keeps read errors visible and recovers only through an explicit retry", async () => {
  mock.fail("/api/preferences", 1, 503, "unavailable", "Settings unavailable");
  save("p-app", "codex", "recovered");
  const { result } = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  await waitFor(() => expect(result.current.error).toBe("Settings unavailable"));
  expect(reads()).toHaveLength(1);
  act(() => result.current.reload());
  await waitFor(() => expect(result.current.model).toBe("recovered"));
  expect(result.current.error).toBeNull();
});

it("reads the saved default again when settings bumps the preferences revision, keeping an explicit choice", async () => {
  save("p-app", "codex", "old");
  const { result } = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  await waitFor(() => expect(result.current.model).toBe("old"));
  save("p-app", "other", "saved");
  act(() => useUi.getState().bumpPreferencesRevision());
  await waitFor(() => expect(result.current.model).toBe("saved"));
  expect(result.current.engine?.id).toBe("other");
  expect(reads()).toHaveLength(2);
  act(() => result.current.choose({ engine: "codex", model: "picked", effort: null }));
  save("p-app", "other", "newer");
  act(() => useUi.getState().bumpPreferencesRevision());
  await waitFor(() => expect(reads()).toHaveLength(3));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.model).toBe("picked"); expect(result.current.engine?.id).toBe("codex");
});

it("takes the project's default effort with its default model over the remembered effort", async () => {
  save("p-app", "codex", "remembered", { default_effort: "low" });
  const { result, unmount } = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  await waitFor(() => expect(result.current.loading).toBe(false));
  expect(result.current.model).toBe("remembered"); expect(result.current.effort).toBe("low");
  unmount();
  // No default effort leaves the remembered one for the same model.
  save("p-app", "codex", "remembered", { default_effort: "" });
  const second = renderHook(() => useDraftModelDefaults("p-app", engines, remembered));
  await waitFor(() => expect(second.result.current.loading).toBe(false));
  expect(second.result.current.effort).toBe("high");
});
