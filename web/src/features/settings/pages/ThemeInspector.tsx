import { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { THEME_TOKENS } from "../../../styles/themes";
import { Button } from "../../../ui/controls";
import { inspectElement, pageEnv, type Inspected } from "./inspectColour";

type Found = Extract<Inspected, { kind: "token" }>;

/**
 * Picking mode of the theme editor (parity K15). Settings is hidden by `html[data-inspecting]` while it runs, so the app behind it
 * can be pointed at; the editor stays mounted and comes back as it was. A click is stopped before it reaches the app, and nothing the
 * app holds is changed: the layer only reads the page's computed colours.
 */
export function ThemeInspectorLayer({ onPick, onCancel }: { onPick(found: Found): void; onCancel(): void }) {
  const [hover, setHover] = useState<{ rect: DOMRect; text: string } | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const done = useRef({ onPick, onCancel });
  done.current = { onPick, onCancel };
  useEffect(() => {
    const root = document.documentElement;
    root.dataset.inspecting = "true";
    const env = pageEnv(THEME_TOKENS);
    const outside = (target: EventTarget | null): target is Element => target instanceof Element && !target.closest("[data-inspector-ui]");
    const move = (event: MouseEvent) => {
      if (!outside(event.target)) { setHover(null); return; }
      const found = inspectElement(event.target, env);
      const text = found.kind === "token" ? `${found.label}, ${found.token}` : found.kind === "content" ? "Content colour, not the theme" : "No theme colour";
      setHover({ rect: event.target.getBoundingClientRect(), text });
    };
    const click = (event: MouseEvent) => {
      if (!outside(event.target)) return;
      event.preventDefault();
      event.stopPropagation();
      const found = inspectElement(event.target, env);
      if (found.kind === "token") done.current.onPick(found);
      else setNote(found.kind === "content" ? found.note : "No theme colour matches that part. Try another.");
    };
    const swallow = (event: Event) => { if (outside(event.target)) { event.preventDefault(); event.stopPropagation(); } };
    const key = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      event.stopPropagation();
      done.current.onCancel();
    };
    document.addEventListener("mousemove", move, true);
    document.addEventListener("click", click, true);
    document.addEventListener("mousedown", swallow, true);
    document.addEventListener("mouseup", swallow, true);
    document.addEventListener("keydown", key, true);
    return () => {
      delete root.dataset.inspecting;
      document.removeEventListener("mousemove", move, true);
      document.removeEventListener("click", click, true);
      document.removeEventListener("mousedown", swallow, true);
      document.removeEventListener("mouseup", swallow, true);
      document.removeEventListener("keydown", key, true);
    };
  }, []);
  return createPortal(
    <div className="inspector-layer" data-inspector-ui>
      <div className="inspector-dim" aria-hidden />
      {hover ? (
        <>
          <div className="inspector-outline" aria-hidden style={{ left: hover.rect.left, top: hover.rect.top, width: hover.rect.width, height: hover.rect.height }} />
          <div className="inspector-tip" aria-hidden style={{ left: hover.rect.left, top: Math.max(4, hover.rect.top - 26) }}>{hover.text}</div>
        </>
      ) : null}
      <section className="inspector-bar" role="region" aria-label="Pick a colour">
        <span>{note ?? "Click a part of the app to find its colour."}</span>
        <Button small onClick={() => done.current.onCancel()}>Cancel</Button>
        <kbd>Esc</kbd>
      </section>
    </div>,
    document.body,
  );
}

/** What was picked, which colour it maps to and how widely that colour is used. */
export function PickedCard({ found, custom, onAgain, onReset }: { found: Found; custom: boolean; onAgain(): void; onReset(): void }) {
  const part = found.part === "text" ? "the text of" : found.part === "border" ? "the border of" : "the background of";
  return (
    <section className="inspector-card" role="status" aria-label="Picked colour">
      <p className="inspector-card-head">
        <b>{found.label}</b>
        <code>{found.token}</code>
        <span className="set-back-end">
          <Button small onClick={onAgain}>Pick again</Button>
          <Button small disabled={!custom} onClick={onReset}>Reset this colour</Button>
        </span>
      </p>
      <p>
        {`You picked ${part} ${found.name}. It uses ${found.label}`}
        {found.sharedWith > 0
          ? `, which ${found.sharedWith} other ${found.sharedWith === 1 ? "part" : "parts"} of the app use as well. Changing it changes all of them.`
          : ". Nothing else uses it. Changing the colour below changes only this."}
      </p>
    </section>
  );
}
