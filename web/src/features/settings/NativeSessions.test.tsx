import { StrictMode } from "react";
import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { EngineStatus } from "../../api/types";
import { useWorkspace } from "../../state/workspace";
import { NativeSessions } from "./NativeSessions";

const engine = { id: "acp-one", tier: "acp", installed: true } as EngineStatus;
const capabilities = { can_list: true, can_load: true, can_resume: false, can_delete: false };
const json = (body: unknown, status = 200) => new Response(JSON.stringify(body), { status, headers: { "Content-Type": "application/json" } });

beforeEach(() => {
  useWorkspace.setState({ projects: { data: [{ id: "project", name: "Project", path: "/project", trusted: true, is_git: true, created_at: "2026-01-01", managed: false, scratch: false }], error: null, loading: false } });
});
afterEach(() => { vi.unstubAllGlobals(); });

it("keeps the live Strict Mode capability read when a cancelled browser request still occupies the engine", async () => {
  let occupied = false;
  let admissions = 0;
  let release: (() => void) | undefined;
  const fetchMock = vi.fn((_input: RequestInfo | URL, init?: RequestInit) => {
    if (occupied) return Promise.resolve(json({ code: "engine_busy", message: "Engine is busy" }, 409));
    occupied = true;
    admissions += 1;
    return new Promise<Response>((resolve, reject) => {
      init?.signal?.addEventListener("abort", () => reject(new DOMException("Aborted", "AbortError")), { once: true });
      release = () => { occupied = false; resolve(json(capabilities)); };
    });
  });
  vi.stubGlobal("fetch", fetchMock);
  render(<StrictMode><NativeSessions engine={engine} /></StrictMode>);
  await act(async () => { await Promise.resolve(); });
  expect(release).toBeDefined();
  await act(async () => { release?.(); });
  expect(await screen.findByRole("button", { name: "List sessions" })).toBeVisible();
  expect(admissions).toBe(1);
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("loads capabilities on an ordinary mount", async () => {
  const fetchMock = vi.fn().mockResolvedValue(json(capabilities));
  vi.stubGlobal("fetch", fetchMock);
  render(<NativeSessions engine={engine} />);
  expect(await screen.findByRole("button", { name: "List sessions" })).toBeVisible();
  expect(fetchMock).toHaveBeenCalledTimes(1);
});

it("hides sessions when the engine cannot list them", async () => {
  const fetchMock = vi.fn().mockResolvedValue(json({ ...capabilities, can_list: false }));
  vi.stubGlobal("fetch", fetchMock);
  render(<NativeSessions engine={engine} />);
  await act(async () => { await Promise.resolve(); });
  expect(fetchMock).toHaveBeenCalledTimes(1);
  expect(screen.queryByRole("button", { name: "List sessions" })).toBeNull();
});

it("does not dispatch a queued request after unmount", async () => {
  const fetchMock = vi.fn().mockResolvedValue(json(capabilities));
  vi.stubGlobal("fetch", fetchMock);
  const view = render(<NativeSessions engine={engine} />);
  view.unmount();
  await act(async () => { await Promise.resolve(); });
  expect(fetchMock).not.toHaveBeenCalled();
});

it("ignores a prior engine response delivered after the engine changes", async () => {
  let resolveOld: ((value: Response) => void) | undefined;
  let resolveNew: ((value: Response) => void) | undefined;
  // The transport deliberately ignores abort to exercise the completion fence.
  const fetchMock = vi.fn((input: RequestInfo | URL) => new Promise<Response>((resolve) => {
    if (String(input).includes("acp-one")) resolveOld = resolve;
    else resolveNew = resolve;
  }));
  vi.stubGlobal("fetch", fetchMock);
  const view = render(<NativeSessions engine={engine} />);
  await act(async () => { await Promise.resolve(); });
  view.rerender(<NativeSessions engine={{ ...engine, id: "acp-two" }} />);
  await act(async () => { await Promise.resolve(); });
  expect(resolveOld).toBeDefined();
  expect(resolveNew).toBeDefined();
  await act(async () => { resolveNew?.(json({ ...capabilities, can_list: false })); });
  await act(async () => { resolveOld?.(json(capabilities)); });
  expect(screen.queryByRole("button", { name: "List sessions" })).toBeNull();
});
