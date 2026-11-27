import { useEffect, useRef } from "react";

/**
 * Escape closes Settings while the page that owns the key is not on the page yet.
 *
 * `SettingsPage` answers Escape itself, with the exclusions a field, menu or dialog needs, and it is a
 * chunk of its own: while that chunk is arriving, or when it did not arrive at all, nothing of the
 * page is up and a person would be held on Settings with no way back. This answers the key instead,
 * with the same dialog guard, which owns Escape even when its focused button has just been disabled.
 * Pass nothing when the screen has no dismissal of its own to hand over to.
 */
export function useCloseOnEscape(onClose: (() => void) | undefined) {
  const latest = useRef(onClose);
  latest.current = onClose;
  const active = onClose !== undefined;
  useEffect(() => {
    if (!active) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      if (document.querySelector("dialog[open], [aria-modal='true']")) return;
      event.preventDefault();
      latest.current?.();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [active]);
}
