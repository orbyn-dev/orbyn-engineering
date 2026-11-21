import { useEffect, useState } from "react";
import { PAGE_MAX, PAGE_MIN, SIZE_RANGE_SENTENCE, parseSize, type PageSize } from "./viewportResize";

type Axis = "width" | "height";

/**
 * The page's width and height as two fields (parity H36). They show the real size, so a drag, a device or Rotate changes them. A
 * typed size resizes the page once, on Enter or when the field is left; a size outside the service's range turns the field red and
 * is never sent. Escape, or leaving the field with a bad value, puts the previous number back.
 */
export function DeviceSizeFields({ size, disabled, onCommit, onProblem }: { size: PageSize; disabled?: boolean; onCommit(next: PageSize): void; onProblem(line: string | null): void }) {
  const [draft, setDraft] = useState<Record<Axis, string>>({ width: String(size.width), height: String(size.height) });
  const [bad, setBad] = useState<Axis | null>(null);
  // A size that arrives from a drag, a device or Rotate replaces what was typed.
  useEffect(() => {
    setDraft({ width: String(size.width), height: String(size.height) });
    setBad(null);
    onProblem(null);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [size.width, size.height]);
  const edit = (axis: Axis, text: string) => {
    setDraft((all) => ({ ...all, [axis]: text }));
    const wrong = text.trim() !== "" && parseSize(text) === null;
    setBad(wrong ? axis : null);
    onProblem(wrong ? SIZE_RANGE_SENTENCE : null);
  };
  const revert = (axis: Axis) => {
    setDraft((all) => ({ ...all, [axis]: String(size[axis]) }));
    setBad(null);
    onProblem(null);
  };
  const commit = (axis: Axis) => {
    const value = parseSize(draft[axis]);
    if (value === null) {
      revert(axis);
      return;
    }
    if (value !== size[axis]) onCommit({ ...size, [axis]: value });
  };
  const field = (axis: Axis, label: string) => (
    <input
      className="browser-size-field"
      type="text"
      inputMode="numeric"
      role="spinbutton"
      aria-label={label}
      aria-valuemin={PAGE_MIN}
      aria-valuemax={PAGE_MAX}
      aria-valuenow={size[axis]}
      disabled={disabled}
      aria-invalid={bad === axis ? true : undefined}
      value={draft[axis]}
      onChange={(event) => edit(axis, event.target.value)}
      onBlur={() => commit(axis)}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          commit(axis);
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          revert(axis);
        } else if (event.key === "ArrowUp" || event.key === "ArrowDown") {
          event.preventDefault();
          const now = parseSize(draft[axis]) ?? size[axis];
          const step = (event.shiftKey ? 10 : 1) * (event.key === "ArrowUp" ? 1 : -1);
          edit(axis, String(Math.min(PAGE_MAX, Math.max(PAGE_MIN, now + step))));
        }
      }}
    />
  );
  return (
    <>
      {field("width", "Width")}
      <span aria-hidden="true">×</span>
      {field("height", "Height")}
    </>
  );
}
