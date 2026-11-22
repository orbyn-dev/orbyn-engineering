/**
 * The Snap Shot runtime: reads the shell's state once, keeps the eight settings in the shell on every change, follows the
 * seven events, and runs one capture at a time. The PNG lives in this module's memory until the composer uploads it; no bytes
 * reach storage, a toast or a log. The page starts a capture only from `captureFromPerson` (the button) or the shell's
 * `snapshot://requested` event (the shortcut), and the result is always a draft the person sends.
 */
import { create } from "zustand";
import { navigate } from "../../product/state/route";
import { useUi } from "../../product/state/ui";
import { readSnapShotGate, type SnapShotClient } from "./shell";
import {
  EXCLUDED_TEXT,
  SNAPSHOT_SETUP_AVAILABLE,
  withoutAccessibleText,
  FAILURE_COPY,
  SNAPSHOT_EVENTS,
  fileOfTake,
  snapshotSettingsOf,
  type SnapShotDraft,
  type SnapShotEventName,
  type SnapShotEventPayload,
  type SnapShotFailureReason,
  type SnapShotSetupAction,
  type SnapShotState,
  type SnapShotTarget,
} from "./types";

const FLASH_MS = 120;
const TOAST_MS = 6000;
const MAX_BUFFERED_EVENT_IDS = 16;
const BUFFERED_EVENT_TTL_MS = 10_000;

type SetupFocus = "all" | "accessibility";
type WaitingFor = "screen_recording" | "accessibility" | null;

interface Capture { id: string; target: SnapShotTarget }
interface BufferedEvent { at: number; events: { event: SnapShotEventName; payload: unknown }[] }

interface SnapshotRuntimeState {
  status: "idle" | "hidden" | "ready";
  client: SnapShotClient | null;
  state: SnapShotState | null;
  target: SnapShotTarget | null;
  capture: Capture | null;
  setupOpen: boolean;
  setupFocus: SetupFocus;
  waitingFor: WaitingFor;
  testCaptureWorked: boolean;
  testDraft: SnapShotDraft | null;
  flashing: boolean;
  /** What the shell last answered to `snapshot_configure`, so the same settings are not sent twice. */
  lastSent: string | null;
  start(): Promise<void>;
  refreshState(): Promise<void>;
  configureNow(): Promise<void>;
  registerTarget(target: SnapShotTarget | null): () => void;
  captureFromPerson(): Promise<void>;
  discard(id: string): Promise<void>;
  receiveEvent(event: SnapShotEventName, payload?: unknown): void;
  openSetup(focus?: SetupFocus): void;
  closeSetup(): void;
  setupAction(action: SnapShotSetupAction): Promise<void>;
  testCapture(): Promise<void>;
  clearTestDraft(): void;
  removeTestText(): void;
}

let started: Promise<void> | null = null;
let configureQueue: Promise<void> = Promise.resolve();
let unlisten: (() => void)[] = [];
let unsubscribePrefs: (() => void) | null = null;
let flashTimer: ReturnType<typeof setTimeout> | null = null;
/** The settings the runtime last put on the wire, so a re-render or another pref does not resend them. */
let sent = "";
let acceptForeignCapture = false;
let captureInFlight = false;
let pending: SnapShotDraft[] = [];
let discardedIds = new Set<string>();
let bufferedEvents = new Map<string, BufferedEvent>();
let quietPreview = false;

/** Drops expired buffers and keeps only the newest ids; events may arrive before `snapshot_capture` names the id. */
const pruneBufferedEvents = (at = Date.now()): void => {
  for (const [id, entry] of bufferedEvents) {
    if (at - entry.at >= BUFFERED_EVENT_TTL_MS) bufferedEvents.delete(id);
  }
  while (bufferedEvents.size > MAX_BUFFERED_EVENT_IDS) {
    const oldest = bufferedEvents.keys().next().value;
    if (oldest === undefined) break;
    bufferedEvents.delete(oldest);
  }
};

const bufferEvent = (id: string, event: SnapShotEventName, payload: unknown): void => {
  pruneBufferedEvents();
  const entry = bufferedEvents.get(id);
  if (entry) entry.events.push({ event, payload });
  else bufferedEvents.set(id, { at: Date.now(), events: [{ event, payload }] });
  pruneBufferedEvents();
};

const takeBufferedEvents = (id: string): { event: SnapShotEventName; payload: unknown }[] => {
  pruneBufferedEvents();
  const entry = bufferedEvents.get(id);
  bufferedEvents.delete(id);
  return entry?.events ?? [];
};

const settleBufferedEvents = (id: string): void => {
  bufferedEvents.delete(id);
};

const settingsNow = () => snapshotSettingsOf(useUi.getState());
const serialized = () => JSON.stringify(settingsNow());

const toast = (text: string, kind: "done" | "failed" = "done", action?: { label: string; run(): void }) =>
  quietPreview ? 0 : useUi.getState().pushToast(text, kind, action, { durationMs: TOAST_MS });

/** The dev-only preview draws a frame without the toast that precedes it; the app never sets this. */
export function setSnapshotPreviewQuiet(quiet: boolean): void {
  quietPreview = quiet;
}

const failureToast = (reason: SnapShotFailureReason) => {
  const copy = FAILURE_COPY[reason] ?? FAILURE_COPY.capture_error;
  const action = !SNAPSHOT_SETUP_AVAILABLE
    ? undefined
    : copy.action === "allow_screen_recording"
      ? { label: "Allow Screen Recording", run: () => void useSnapshot.getState().setupAction("allow_screen_recording") }
      : copy.action === "setup"
        ? { label: "Set up Snap Shot…", run: () => useSnapshot.getState().openSetup("all") }
        : undefined;
  toast(`${copy.title}. ${copy.body}`, "failed", action);
};

const flash = () => {
  if (!useUi.getState().snapshotFlash) return;
  const reduced = typeof matchMedia === "function" && matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (reduced) return;
  if (flashTimer) clearTimeout(flashTimer);
  useSnapshot.setState({ flashing: true });
  flashTimer = setTimeout(() => {
    flashTimer = null;
    useSnapshot.setState({ flashing: false });
  }, FLASH_MS);
};

export const useSnapshot = create<SnapshotRuntimeState>((set, get) => ({
  status: "idle",
  client: null,
  state: null,
  target: null,
  capture: null,
  setupOpen: false,
  setupFocus: "all",
  waitingFor: null,
  testCaptureWorked: false,
  testDraft: null,
  flashing: false,
  lastSent: null,

  async start() {
    if (started) return started;
    started = (async () => {
      if (get().status !== "idle") return;
      const gate = await readSnapShotGate();
      if (!gate) {
        set({ status: "hidden" });
        return;
      }
      set({ status: "ready", client: gate.client, state: gate.state });
      const stops: (() => void)[] = [];
      for (const event of SNAPSHOT_EVENTS) {
        try {
          stops.push(await gate.client.on(event, (payload) => get().receiveEvent(event, payload)));
        } catch {
          // A shell that cannot carry events still answers the commands; the shortcut path is simply absent.
        }
      }
      unlisten = stops;
      unsubscribePrefs = useUi.subscribe(() => {
        if (get().status !== "ready") return;
        const next = serialized();
        if (next === sent) return;
        sent = next;
        void get().configureNow();
      });
      sent = serialized();
      await get().configureNow();
      // Turning the setting on opens the setup dialog once; the shell registers nothing while it is off.
      const state = get().state;
      if (state?.enabled && state.needs_setup !== null) get().openSetup(state.needs_setup === "accessibility" ? "accessibility" : "all");
    })();
    return started;
  },

  async refreshState() {
    const { client, status } = get();
    if (!client || status !== "ready") return;
    try {
      const state = await client.state();
      set({ state, status: state.supported ? "ready" : "hidden" });
    } catch {
      // A failed read changes nothing; the next focus or state_changed reads again.
    }
  },

  registerTarget(target) {
    set({ target });
    const held = pending;
    pending = [];
    for (const draft of held) {
      if (!target) break;
      void target.receive(draft).catch(() => undefined);
    }
    return () => {
      if (get().target === target) set({ target: null });
    };
  },

  async captureFromPerson() {
    const { status, client, state, capture, target } = get();
    if (status !== "ready" || !client || !state?.enabled || !useUi.getState().snapshotEnabled || capture || captureInFlight || !target) return;
    captureInFlight = true;
    try {
      pruneBufferedEvents();
      const { id } = await client.capture();
      if (!target) {
        settleBufferedEvents(id);
        await client.discard(id).catch(() => undefined);
        return;
      }
      set({ capture: { id, target } });
      target.phase("capturing");
      // The shell emits `started` and a fast `failed` before `snapshot_capture` answers the id; replay them now.
      for (const held of takeBufferedEvents(id)) get().receiveEvent(held.event, held.payload);
    } catch {
      failureToast("capture_error");
    } finally {
      captureInFlight = false;
    }
  },

  async discard(id) {
    discardedIds.add(id);
    settleBufferedEvents(id);
    const { client, capture, target } = get();
    if (capture?.id === id) set({ capture: null });
    if (get().testDraft?.captureId === id) set({ testDraft: null });
    target?.phase(null);
    if (!client) return;
    await client.discard(id).catch(() => undefined);
  },

  receiveEvent(event, payload) {
    const data = (typeof payload === "object" && payload !== null ? payload : {}) as SnapShotEventPayload;
    const id = typeof data.id === "string" ? data.id : null;
    if (id !== null && discardedIds.has(id)) {
      settleBufferedEvents(id);
      return;
    }
    const runtime = get();
    if (event === "snapshot://requested") {
      void runtime.captureFromPerson();
      return;
    }
    if (event === "snapshot://state_changed") {
      void runtime.refreshState();
      return;
    }
    const current = runtime.capture;
    if (event === "snapshot://excluded") {
      if (id !== null && current?.id !== id && !(acceptForeignCapture && current === null)) {
        if (current === null) bufferEvent(id, event, payload);
        return;
      }
      if (id !== null) settleBufferedEvents(id);
      else if (current) settleBufferedEvents(current.id);
      acceptForeignCapture = false;
      const name = typeof data.app_name === "string" ? data.app_name : null;
      current?.target.phase(null);
      set({ capture: null });
      toast(
        name ? `Not captured: ${name}. ${EXCLUDED_TEXT}` : EXCLUDED_TEXT,
        "failed",
        { label: "Open Settings", run: () => navigate({ name: "settings", section: "general" }) },
      );
      return;
    }
    if (id === null) return;
    if (current?.id !== id && !(acceptForeignCapture && current === null)) {
      if (current === null) bufferEvent(id, event, payload);
      return;
    }
    if (event === "snapshot://failed") {
      acceptForeignCapture = false;
      settleBufferedEvents(id);
      if (current) current.target.phase(null);
      set({ capture: null });
      const reason = typeof data.reason === "string" ? data.reason : "capture_error";
      failureToast(reason);
      return;
    }
    if (event !== "snapshot://ready") return;
    const wasTest = acceptForeignCapture;
    acceptForeignCapture = false;
    settleBufferedEvents(id);
    if (wasTest) set({ testCaptureWorked: true });
    void takeAndDeliver(id, current?.target ?? runtime.target, wasTest);
  },

  openSetup(focus = "all") {
    if (!SNAPSHOT_SETUP_AVAILABLE) return;
    set({ setupOpen: true, setupFocus: focus });
  },

  closeSetup() {
    set({ setupOpen: false, waitingFor: null });
  },

  async setupAction(action) {
    if (!SNAPSHOT_SETUP_AVAILABLE && (action === "allow_screen_recording" || action === "allow_accessibility")) return;
    const { client, state } = get();
    if (!client) return;
    if (action === "test_capture") {
      await get().testCapture();
      return;
    }
    try {
      const answer = await client.setup(action);
      set({ state: answer });
      if (action === "allow_screen_recording") {
        set({ waitingFor: answer.permissions.screen_recording ? null : "screen_recording" });
      } else if (action === "allow_accessibility") {
        set({ waitingFor: answer.permissions.accessibility ? null : "accessibility" });
      }
      if (!answer.supported) set({ status: "hidden" });
      if (state === null) set({ state: answer });
    } catch {
      // The dialog keeps showing the last state it read; nothing is claimed.
    }
  },

  async testCapture() {
    const { client, state } = get();
    if (!client || !state?.enabled || !useUi.getState().snapshotEnabled) return;
    acceptForeignCapture = true;
    set({ testCaptureWorked: false, testDraft: null });
    try {
      const answer = await client.setup("test_capture");
      set({ state: answer });
    } catch {
      acceptForeignCapture = false;
      failureToast("capture_error");
    }
  },

  clearTestDraft() {
    set({ testDraft: null });
  },

  removeTestText() {
    const draft = get().testDraft;
    if (draft) set({ testDraft: { ...draft, source: withoutAccessibleText(draft.source) } });
  },

  async configureNow() {
    const { client, status } = get();
    if (!client || status !== "ready") return;
    const settings = settingsNow();
    configureQueue = configureQueue.then(async () => {
      try {
        const state = await client.configure(settings);
        set({ state, status: state.supported ? "ready" : "hidden", lastSent: JSON.stringify(settings) });
      } catch {
        // The shell keeps what it had; the state still says what it last read.
      }
    });
    await configureQueue;
  },
}));

/** Runs one ready capture through take and into the target. */
async function takeAndDeliver(id: string, target: SnapShotTarget | null, wasTest = false) {
  const client = useSnapshot.getState().client;
  if (!client) return;
  let take;
  try {
    take = await client.take(id);
  } catch {
    useSnapshot.setState({ capture: null });
    target?.phase(null);
    failureToast("capture_error");
    return;
  }
  const file = fileOfTake(take);
  const preview = URL.createObjectURL(file);
  const draft: SnapShotDraft = {
    captureId: id,
    file,
    name: take.name,
    preview,
    source: take.source,
  };
  const destination = target ?? useSnapshot.getState().target;
  if (!destination) {
    if (wasTest) useSnapshot.setState({ testDraft: draft });
    else pending.push(draft);
    useSnapshot.setState({ capture: null });
    flash();
    return;
  }
  try {
    await destination.receive(draft);
    useSnapshot.setState({ capture: null });
    destination.phase(null);
    const app = take.source.app_name;
    toast(
      destination.kind === "draft"
        ? `Captured ${app} · no thread is open, so it is a draft in the new-thread composer. Nothing is sent.`
        : `Captured ${app} · it is a draft in ${destination.label}. Nothing is sent until you send it.`,
    );
    flash();
  } catch {
    URL.revokeObjectURL(preview);
    await client.discard(id).catch(() => undefined);
    useSnapshot.setState({ capture: null });
    destination.phase(null);
    failureToast("capture_error");
  }
}

/** Forgets the runtime between tests: listeners, queued settings, pending drafts and the flash timer. */
export function resetSnapshotRuntimeForTests(): void {
  unlisten.forEach((stop) => stop());
  unlisten = [];
  unsubscribePrefs?.();
  unsubscribePrefs = null;
  if (flashTimer) clearTimeout(flashTimer);
  flashTimer = null;
  started = null;
  configureQueue = Promise.resolve();
  sent = "";
  acceptForeignCapture = false;
  captureInFlight = false;
  pending = [];
  discardedIds = new Set<string>();
  bufferedEvents = new Map<string, BufferedEvent>();
  quietPreview = false;
  useSnapshot.setState(useSnapshot.getInitialState(), true);
}
