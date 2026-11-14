// What it takes to approve one held command: the person's own click, in the desktop app. The native dialog that
// confirms it is drawn by the shell; this file holds the page's side of that, which is only the rules for when it may ask.
import { useEffect, useState } from "react";
import { canAskPresence, onShellChange, readShellStatus } from "../../../../api/shell";

/** The click a handler got: the part of a React click event this file reads. */
export interface Gesture {
  nativeEvent: Pick<UIEvent, "isTrusted" | "type" | "detail">;
}

/**
 * True only for a click the browser made from the person's own input. A script's `dispatchEvent` or `click()` is not
 * trusted, and a key event is not a click. A click made by keyboard (Enter or Space on a focused button) is trusted:
 * the person pressed that button, so it counts. Nothing but a click reaches `crew_presence`.
 */
export const isPersonsClick = (gesture: Gesture): boolean =>
  gesture.nativeEvent.isTrusted === true && gesture.nativeEvent.type === "click";

let enterDown = false;
let enterWaiters: (() => void)[] = [];
if (typeof window !== "undefined") {
  // Only a flag: nothing here asks the shell. It lets a keyboard click wait until Enter is up.
  window.addEventListener("keydown", (event) => { if (event.key === "Enter") enterDown = true; }, true);
  window.addEventListener("keyup", (event) => {
    if (event.key !== "Enter") return;
    enterDown = false;
    const waiting = enterWaiters;
    enterWaiters = [];
    waiting.forEach((resolve) => resolve());
  }, true);
  window.addEventListener("blur", () => { enterDown = false; });
}

/** How long to wait for Enter to come up before going on anyway. */
export const ENTER_WAIT_MS = 2000;

/**
 * A click made by pressing Enter fires on the key's press, and a held key repeats. The shell's dialog approves on Return, so
 * the page waits for the key to come up before it asks, and the repeats are over when the dialog opens. A mouse click
 * (`detail` 1 or more) and Space (which clicks on release) go straight on.
 */
export function afterKeyboardRelease(gesture: Gesture, wait = ENTER_WAIT_MS): Promise<void> {
  if (gesture.nativeEvent.detail !== 0 || !enterDown) return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => { clearTimeout(timer); enterWaiters = enterWaiters.filter((one) => one !== done); resolve(); };
    const timer = setTimeout(done, wait);
    enterWaiters.push(done);
  });
}

/**
 * Whether this page is in the desktop app and can ask. It starts from what is known and reads the shell's status once,
 * as the title bar does, so a card drawn before the first answer corrects itself.
 */
export function useDesktopApp(): boolean {
  const [desktop, setDesktop] = useState(canAskPresence);
  useEffect(() => {
    let live = true;
    const read = () => void readShellStatus().then(() => live && setDesktop(canAskPresence()), () => undefined);
    read();
    const stop = onShellChange(read);
    return () => {
      live = false;
      stop();
    };
  }, []);
  return desktop;
}
