// What it takes to approve one held command: the person's own click, in the desktop app. The native dialog that
// confirms it is drawn by the shell; this file holds the page's side of that, which is only the rules for when it may ask.
import { useEffect, useState } from "react";
import { canAskPresence, onShellChange, readShellStatus } from "../../../../api/shell";

/** The click a handler got: the part of a React click event this file reads. */
export interface Gesture {
  nativeEvent: Pick<Event, "isTrusted" | "type">;
}

/**
 * True only for a click the browser made from the person's own input. A script's `dispatchEvent` or `click()` is not
 * trusted, and a key event is not a click. A click made by keyboard (Enter or Space on a focused button) is trusted:
 * the person pressed that button, so it counts. Nothing but a click reaches `crew_presence`.
 */
export const isPersonsClick = (gesture: Gesture): boolean =>
  gesture.nativeEvent.isTrusted === true && gesture.nativeEvent.type === "click";

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
