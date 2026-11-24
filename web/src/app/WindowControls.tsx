import { useEffect, useRef, useState } from "react";
import { closeWindow, isWindowMaximized, maximizeWindow, minimizeWindow, setMaximizeRect } from "./windowControlsApi";

const GLYPH_PX = 13;

function MinimizeGlyph() {
  return (
    <svg width={GLYPH_PX} height={GLYPH_PX} viewBox="0 0 13 13" fill="none" aria-hidden="true" focusable="false">
      <path d="M3 6.5h7" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

function MaximizeGlyph() {
  return (
    <svg width={GLYPH_PX} height={GLYPH_PX} viewBox="0 0 13 13" fill="none" aria-hidden="true" focusable="false">
      <rect x="2.75" y="2.75" width="7.5" height="7.5" rx="0.5" stroke="currentColor" strokeWidth="1.1" />
    </svg>
  );
}

function RestoreGlyph() {
  return (
    <svg width={GLYPH_PX} height={GLYPH_PX} viewBox="0 0 13 13" fill="none" aria-hidden="true" focusable="false">
      <rect x="4.25" y="2.25" width="6.5" height="6.5" rx="0.5" stroke="currentColor" strokeWidth="1.1" />
      <path d="M2.25 4.25h6.5v6.5h-6.5z" stroke="currentColor" strokeWidth="1.1" />
    </svg>
  );
}

function CloseGlyph() {
  return (
    <svg width={GLYPH_PX} height={GLYPH_PX} viewBox="0 0 13 13" fill="none" aria-hidden="true" focusable="false">
      <path d="m3 3 7 7M10 3l-7 7" stroke="currentColor" strokeWidth="1.2" strokeLinecap="round" />
    </svg>
  );
}

/**
 * The Windows integrated toolbar's window controls, at the bar's top-right corner. They are drawn
 * by the page, not the system: each command goes to the shell through the bridge, and a failed
 * command changes nothing. Maximize reports its box so the shell can answer Windows' hit test.
 */
export function WindowControls() {
  const [maximized, setMaximized] = useState(false);
  const [inactive, setInactive] = useState(false);
  const maximize = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    let live = true;
    const measure = () => {
      const node = maximize.current;
      if (!node) return;
      const rect = node.getBoundingClientRect();
      // Device pixels: the window's zoom changes what a CSS pixel is, and the shell reads the mouse in device pixels.
      const ratio = window.devicePixelRatio || 1;
      setMaximizeRect({
        x: rect.x * ratio,
        y: rect.y * ratio,
        width: rect.width * ratio,
        height: rect.height * ratio,
      });
    };
    const read = () => {
      void isWindowMaximized().then((value) => {
        if (live) setMaximized(value);
      });
    };
    const sync = () => {
      read();
      measure();
    };
    sync();
    window.addEventListener("resize", sync);
    const watch = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    if (maximize.current) watch?.observe(maximize.current);
    return () => {
      live = false;
      window.removeEventListener("resize", sync);
      watch?.disconnect();
      setMaximizeRect(null);
    };
  }, []);

  useEffect(() => {
    const focus = () => setInactive(false);
    const blur = () => setInactive(true);
    window.addEventListener("focus", focus);
    window.addEventListener("blur", blur);
    return () => {
      window.removeEventListener("focus", focus);
      window.removeEventListener("blur", blur);
    };
  }, []);

  return (
    <div
      className="window-controls"
      role="group"
      aria-label="Window controls"
      data-inactive={inactive || undefined}
      data-maximized={maximized || undefined}
    >
      <button type="button" className="window-control" aria-label="Minimize" onClick={minimizeWindow}>
        <MinimizeGlyph />
      </button>
      <button
        ref={maximize}
        type="button"
        className="window-control"
        aria-label={maximized ? "Restore" : "Maximize"}
        onClick={maximizeWindow}
      >
        {maximized ? <RestoreGlyph /> : <MaximizeGlyph />}
      </button>
      <button type="button" className="window-control window-control-close" aria-label="Close" onClick={closeWindow}>
        <CloseGlyph />
      </button>
    </div>
  );
}
