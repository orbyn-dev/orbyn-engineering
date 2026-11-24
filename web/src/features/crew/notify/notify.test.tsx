import { act, render } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { useConnection } from "../../../product/state/connection";
import { useUi } from "../../../product/state/ui";
import { installMockFetch } from "../../../test-support";
import { CREW_NOTIFY_FIRST_POLL_MS, CREW_NOTIFY_WINDOW_MS, useCrewNotifications } from "./useCrewNotifications";

function Probe() {
  useCrewNotifications();
  return null;
}

/** The system notification the hook makes: the title, the options and the click it installs. */
class SentNotification {
  static permission: NotificationPermission = "granted";
  onclick: (() => void) | null = null;
  readonly close = vi.fn();
  constructor(readonly title: string, readonly options?: { body?: string; tag?: string }) {
    sent.push(this);
  }
}

let sent: SentNotification[] = [];
let mock: MockApi;

beforeEach(() => {
  vi.useFakeTimers();
  mock = installMockFetch();
  sent = [];
  SentNotification.permission = "granted";
  useUi.setState({ toasts: [], crewNotifications: true, threadNotifications: true });
  // The app's own info read says Crew Preview is on; the hook adds no preview request of its own.
  useConnection.setState({ crewPreview: true });
  vi.stubGlobal("Notification", SentNotification);
  vi.spyOn(document, "hasFocus").mockReturnValue(false);
  window.location.hash = "";
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** Settles the reads and state updates a timer callback starts. */
const flush = () =>
  act(async () => {
    for (let step = 0; step < 10; step += 1) await Promise.resolve();
  });

/** Advances fake time and lets the requests it starts finish. */
const advance = async (ms: number) => {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
  await flush();
};

async function open(scenario: string) {
  mock.reset(scenario as Parameters<MockApi["reset"]>[0]);
  render(<Probe />);
  await flush();
}

const calls = (path: string, method: string) =>
  vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url) === path && (init?.method ?? "GET") === method);
const needsYouReads = () => calls("/api/crew/needs-you", "GET");
const notifyPosts = () => calls("/api/crew/needs-you/notify", "POST");
const posted = (call: [RequestInfo | URL, RequestInit?] | undefined) => JSON.parse(String(call?.[1]?.body)) as { items: { kind: string; id: string }[] };

describe("Crew device notifications", () => {
  it("offers a new question once after the window and raises one card with its text", async () => {
    await open("crew-notify-one");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    expect(notifyPosts()).toHaveLength(0);
    expect(sent).toHaveLength(0);

    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(notifyPosts()).toHaveLength(1);
    expect(posted(notifyPosts()[0])).toEqual({ items: [{ kind: "task_needs_person", id: "task-q1" }] });
    expect(sent).toHaveLength(1);
    expect(sent[0]?.title).toBe("Asked you a question");
    expect(sent[0]?.options?.body).toBe("Which target wins when both next and returnTo are set?");
    expect(sent[0]?.options?.tag).toBe("orbyn-crew:task_needs_person:task-q1");

    const focus = vi.spyOn(window, "focus").mockImplementation(() => undefined);
    await act(async () => sent[0]?.onclick?.());
    expect(focus).toHaveBeenCalled();
    expect(window.location.hash).toBe("#/crew/inbox");
    expect(sent[0]?.close).toHaveBeenCalled();
  });

  it("raises ONE grouped card for five items that arrive together", async () => {
    await open("crew-notify-five");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(notifyPosts()).toHaveLength(1);
    expect(posted(notifyPosts()[0]).items).toHaveLength(5);
    expect(sent).toHaveLength(1);
    expect(sent[0]?.title).toBe("Crew: 5 things need you");
    expect(sent[0]?.options?.body).toBe("2 questions · 1 merge · 1 review · 1 watch");
    expect(sent[0]?.options?.tag).toBe("orbyn-crew");
  });

  it("raises two single cards for two items", async () => {
    await open("crew-notify-two");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(sent.map((one) => one.title)).toEqual(["Could not start", "Stopped"]);
    expect(sent.map((one) => one.options?.tag)).toEqual(["orbyn-crew:could_not_start:task-refused", "orbyn-crew:tripwire_stop:task-stopped"]);
  });

  it("raises only the claimed items when another window already has some", async () => {
    await open("crew-notify-claimed");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(posted(notifyPosts()[0]).items).toHaveLength(2);
    expect(sent).toHaveLength(1);
    expect(sent[0]?.title).toBe("Asked you a question");
    expect(sent[0]?.options?.body).toBe("Which target wins when both next and returnTo are set?");
  });

  it("does not offer an item again on a later poll in the same wait", async () => {
    await open("crew-notify-one");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(notifyPosts()).toHaveLength(1);
    expect(sent).toHaveLength(1);

    // The next poll reads the same item again, still at the same wait.
    await advance(30_000);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(needsYouReads()).toHaveLength(2);
    expect(notifyPosts()).toHaveLength(1);
    expect(sent).toHaveLength(1);
  });

  it("asks nothing while the Crew notifications preference is off", async () => {
    useUi.setState({ crewNotifications: false });
    await open("crew-notify-one");
    await advance(60_000);
    expect(calls("/api/crew/preview", "GET")).toHaveLength(0);
    expect(needsYouReads()).toHaveLength(0);
    expect(notifyPosts()).toHaveLength(0);
    expect(sent).toHaveLength(0);
  });

  it("still tells the service when the permission is denied, and raises no card", async () => {
    SentNotification.permission = "denied";
    await open("crew-notify-one");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(notifyPosts()).toHaveLength(1);
    expect(sent).toHaveLength(0);
  });

  it("still tells the service when Thread notifications is off in General, and raises no card", async () => {
    useUi.setState({ threadNotifications: false });
    await open("crew-notify-one");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(notifyPosts()).toHaveLength(1);
    expect(sent).toHaveLength(0);
  });

  it("still tells the service when the window is focused, and raises no card", async () => {
    vi.mocked(document.hasFocus).mockReturnValue(true);
    await open("crew-notify-one");
    await advance(CREW_NOTIFY_FIRST_POLL_MS);
    await advance(CREW_NOTIFY_WINDOW_MS);
    expect(notifyPosts()).toHaveLength(1);
    expect(sent).toHaveLength(0);
  });

  it("does nothing, silently, when the service answers 404 for the read", async () => {
    await open("crew-notify-missing");
    await advance(60_000);
    expect(needsYouReads()).toHaveLength(1);
    expect(notifyPosts()).toHaveLength(0);
    expect(sent).toHaveLength(0);
  });
});
