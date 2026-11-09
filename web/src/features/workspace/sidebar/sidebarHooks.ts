import { useEffect, useState } from "react";
import type { Thread } from "../../../api/types";

/** True while Mod is held on its own, when number badges show on the first rows. */
export function useJumpHeld() {
  const [held, setHeld] = useState(false);
  useEffect(() => {
    const sync = (event: KeyboardEvent) =>
      setHeld((/mac/i.test(navigator.platform) ? event.metaKey : event.ctrlKey) && !event.shiftKey && !event.altKey);
    const clear = () => setHeld(false);
    window.addEventListener("keydown", sync);
    window.addEventListener("keyup", sync);
    window.addEventListener("blur", clear);
    return () => {
      window.removeEventListener("keydown", sync);
      window.removeEventListener("keyup", sync);
      window.removeEventListener("blur", clear);
    };
  }, []);
  return held;
}

export const searchFields = (thread: Thread, project: string) =>
  `${thread.title} ${project} ${thread.worktree?.branch ?? ""}`.toLowerCase();
