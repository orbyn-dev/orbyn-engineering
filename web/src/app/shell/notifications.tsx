import { useEffect } from "react";
import { create } from "zustand";
import type { Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";

/** Read out by screen readers when another thread changes. */
const useAnnouncer = create<{ text: string }>(() => ({ text: "" }));

export function BackgroundAnnouncer() {
  const text = useAnnouncer((state) => state.text);
  return (
    <div className="sr-only" role="status" aria-label="Background threads">
      {text}
    </div>
  );
}

type Attention = "working" | "waiting" | "done" | "failed" | "idle";

const attentionOf = (thread: Thread): Attention =>
  thread.active_run?.state === "awaiting_approval"
    ? "waiting"
    : thread.active_run
      ? "working"
      : thread.last_run_state === "failed"
        ? "failed"
        : thread.last_run_state === "completed"
          ? "done"
          : "idle";

const notices: Partial<Record<Attention, string>> = {
  waiting: "needs you",
  done: "finished",
  failed: "failed",
};

/** A short tone. Browsers refuse audio before a first click, which costs nothing here. */
function playChime() {
  try {
    const audio = new AudioContext();
    const tone = audio.createOscillator();
    const level = audio.createGain();
    tone.frequency.value = 660;
    level.gain.setValueAtTime(0.06, audio.currentTime);
    level.gain.exponentialRampToValueAtTime(0.001, audio.currentTime + 0.18);
    tone.connect(level).connect(audio.destination);
    tone.start();
    tone.stop(audio.currentTime + 0.18);
    tone.onended = () => void audio.close();
  } catch {
    // No audio device. The other notices still go out.
  }
}

/** Tells you when another thread finishes, fails or needs you. */
export function useThreadNotifications(openId: string | null) {
  useEffect(() => {
    const seen = new Map<string, Attention>();
    return useWorkspace.subscribe((state) => {
      for (const thread of Object.values(state.threads)) {
        const now = attentionOf(thread);
        const before = seen.get(thread.id);
        seen.set(thread.id, now);
        const notice = notices[now];
        if (before === undefined || before === now || !notice || thread.id === openId) continue;
        useAnnouncer.setState({ text: `${thread.title} ${notice}` });
        const { inAppNotifications, threadNotifications, threadSound, pushToast } = useUi.getState();
        if (inAppNotifications) pushToast(`${thread.title} ${notice}`, now === "waiting" ? "needs" : now === "done" ? "done" : "failed");
        if (threadSound) playChime();
        if (threadNotifications && document.hidden && "Notification" in window && Notification.permission === "granted") {
          new Notification(thread.title, { body: notice });
        }
      }
    });
  }, [openId]);
}
