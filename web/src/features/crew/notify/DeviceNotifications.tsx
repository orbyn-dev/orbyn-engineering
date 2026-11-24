// Settings › Crew › Device notifications (Crew Designs v140, frames `kc10-notify-settings*`): the switch for KC10's
// system cards, on by default, with one status line that says what the computer and General allow right now. The
// permission is read on every render and again when the window regains focus, so a person who allowed notifications in
// the system's own settings sees the line change when they come back. Nothing here asks for the permission.
import { useEffect, useState } from "react";
import { navigate } from "../../../product/state/route";
import { useUi } from "../../../product/state/ui";

const OFF =
  "Off. Nothing is raised on this computer. Items still show in Needs you, on the board and as toasts in the app. The digest notification has its own switch below.";
const THREADS_OFF =
  "Thread notifications in Settings › General is Off, so no system notification is raised. Choose Notifications or Both there.";
const DENIED =
  "Your computer is blocking notifications from Orbyn. Allow them in System Settings › Notifications (on Windows: Settings › System › Notifications), then come back. Until then Crew items show in Needs you and as toasts in the app.";
const NOT_ASKED = "Not asked yet. Choose Notifications or Both in Settings › General to allow it.";
const ALLOWED = "Allowed by your computer. Thread notifications in Settings › General is on.";

interface Status {
  text: string;
  openGeneral?: boolean;
  muted?: boolean;
}

/** The one line to draw: the switch first, then General, then the permission; an unasked permission only says where to allow it, and a browser with no Notification shows none. */
function statusOf(on: boolean, threads: boolean, permission: NotificationPermission | null): Status | null {
  if (!on) return { text: OFF };
  if (!threads) return { text: THREADS_OFF, openGeneral: true };
  if (permission === "denied") return { text: DENIED };
  if (permission === "granted") return { text: ALLOWED };
  if (permission === "default") return { text: NOT_ASKED, muted: true };
  return null;
}

export function DeviceNotifications() {
  const on = useUi((state) => state.crewNotifications);
  const threads = useUi((state) => state.threadNotifications);
  // A focus event bumps this, so the permission is read again when a person comes back from the system's settings.
  const [, setTick] = useState(0);
  useEffect(() => {
    const reread = () => setTick((tick) => tick + 1);
    window.addEventListener("focus", reread);
    return () => window.removeEventListener("focus", reread);
  }, []);

  const permission = window.Notification ? window.Notification.permission : null;
  const status = statusOf(on, threads, permission);
  return (
    <section className="dn-row" aria-label="Device notifications">
      <div>
        <b>Device notifications</b>
        <small>
          A notification on this computer the first time something needs you: a question, a command to approve, a merge or a review that waits, a watch that turned failing, a start that was refused, a task that stopped. One per item, never repeated; items that arrive together are one. Items still appear in Needs you whatever this says. It uses Thread notifications in Settings › General.
        </small>
      </div>
      <button
        type="button"
        role="switch"
        aria-checked={on}
        aria-label="Device notifications"
        className="ap-switch"
        onClick={() => useUi.getState().setPrefs({ crewNotifications: !on })}
      >
        {on ? "On" : "Off"}
      </button>
      {status ? (
        <p className={status.muted ? "dn-note" : "dn-status"} role="status">
          <span>{status.text}</span>
          {status.openGeneral ? (
            <button type="button" className="btn btn-small" onClick={() => navigate({ name: "settings", section: "general" })}>
              Open General
            </button>
          ) : null}
        </p>
      ) : null}
    </section>
  );
}
