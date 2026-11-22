import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useUi } from "../../../../product/state/ui";
import { setAttentionApi, type AttentionApi, type AttentionDelivery } from "./contract";
import { useAttention } from "./store";
import { useTerminalAttention } from "./useAttention";

function Host({ open = null }: { open?: string | null }) { useTerminalAttention(open); return null; }
const waiting = (over: Record<string, unknown> = {}) => ({ v: 1, op: "waiting", id: "a1", terminal_id: "tm1", terminal_title: "dev", engine: "claude", kind: "question", since: "2026-11-22T14:12:00Z", excerpt: ["Which test command?"], ...over });
const at = (seq: number, value: unknown, thread = "t1"): AttentionDelivery => ({ seq, ts: "2026-11-22T14:12:03Z", thread_id: thread, event: { type: "CUSTOM", name: "orbyn.terminal_attention", value } });

let push: (delivery: AttentionDelivery) => void = () => undefined;
const stop = vi.fn();
const api: AttentionApi = { subscribe(onEvent) { push = onEvent; return stop; } };
const Notice = vi.fn();

beforeEach(() => {
  useAttention.getState().reset();
  useUi.setState({ toasts: [], inAppNotifications: true, threadNotifications: true });
  Notice.mockReset();
  vi.stubGlobal("Notification", Object.assign(Notice, { permission: "granted" }));
  Object.defineProperty(document, "hidden", { value: true, configurable: true });
});
afterEach(() => { cleanup(); setAttentionApi(null); stop.mockReset(); vi.unstubAllGlobals(); Object.defineProperty(document, "hidden", { value: false, configurable: true }); });

describe("useTerminalAttention", () => {
  it("with no client it subscribes to nothing and raises nothing", () => {
    render(<Host />);
    expect(useUi.getState().toasts).toHaveLength(0);
    expect(Notice).not.toHaveBeenCalled();
  });
  it("keeps its subscription when the open thread changes, and a toast for the open thread is left to its item", () => {
    const subscribe = vi.fn(api.subscribe);
    setAttentionApi({ subscribe });
    const view = render(<Host open="t1" />);
    view.rerender(<Host open="t2" />);
    expect(subscribe).toHaveBeenCalledTimes(1);
    expect(stop).not.toHaveBeenCalled();
    act(() => push(at(1, waiting(), "t2")));
    expect(useUi.getState().toasts).toHaveLength(0);
    act(() => push(at(2, waiting({ id: "a2", terminal_id: "tm2" }), "t1")));
    expect(useUi.getState().toasts).toHaveLength(1);
  });
  it("raises one toast and one notification for a new waiting item, none for an update", () => {
    setAttentionApi(api);
    render(<Host />);
    act(() => push(at(1, waiting())));
    act(() => push(at(2, waiting({ excerpt: ["Which test command now?"] }))));
    expect(useUi.getState().toasts).toHaveLength(1);
    expect(useUi.getState().toasts[0]?.text).toBe("Claude Code needs you in “dev”");
    expect(Notice).toHaveBeenCalledTimes(1);
    expect(Notice.mock.calls[0]?.[1]?.body).toContain("Which test command?");
  });
  it("a secret prompt's text is in neither", () => {
    setAttentionApi(api);
    render(<Host />);
    act(() => push(at(1, waiting({ kind: "secret", excerpt: ["Password:"] }))));
    expect(useUi.getState().toasts[0]?.text).not.toContain("Password");
    expect(JSON.stringify(Notice.mock.calls)).not.toContain("Password:");
  });
  it("follows the in-app and system notification settings, and asks for no permission", () => {
    useUi.setState({ inAppNotifications: false, threadNotifications: false });
    const ask = vi.fn();
    Object.assign(Notice, { requestPermission: ask });
    setAttentionApi(api);
    render(<Host />);
    act(() => push(at(1, waiting())));
    expect(useUi.getState().toasts).toHaveLength(0);
    expect(Notice).not.toHaveBeenCalled();
    expect(ask).not.toHaveBeenCalled();
  });
  it("sends no system notification without granted permission, or when the app is shown", () => {
    setAttentionApi(api);
    vi.stubGlobal("Notification", Object.assign(Notice, { permission: "default" }));
    render(<Host />);
    act(() => push(at(1, waiting())));
    expect(Notice).not.toHaveBeenCalled();
  });
  it("one notification per terminal in 30 s", () => {
    setAttentionApi(api);
    render(<Host />);
    act(() => push(at(1, waiting())));
    act(() => push(at(2, { v: 1, op: "clear", id: "a1", reason: "output" })));
    act(() => push(at(3, waiting({ id: "a2" }))));
    expect(Notice).toHaveBeenCalledTimes(1);
  });
  it("unsubscribes on unmount", () => {
    setAttentionApi(api);
    const { unmount } = render(<Host />);
    unmount();
    expect(stop).toHaveBeenCalledTimes(1);
  });
});
