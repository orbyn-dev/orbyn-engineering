import { useEffect, useRef } from "react";

const SIZE = 6;
const digitsOf = (text: string) => text.replace(/\D/g, "").slice(0, SIZE);

/**
 * Six boxes for the six digits shown on the other device. They take digits only, accept a pasted six-digit number
 * (spaces ignored), move on by themselves and back on delete, and open the numeric keypad on a phone or tablet. Nothing
 * is filled in for the person. `value` is the digits typed so far, with no spaces.
 */
export function DigitBoxes({ value, onChange, disabled, bad, resetKey }: { value: string; onChange(digits: string): void; disabled?: boolean; bad?: boolean; resetKey?: unknown }) {
  const boxes = useRef<Array<HTMLInputElement | null>>([]);
  const focus = (index: number) => boxes.current[Math.max(0, Math.min(SIZE - 1, index))]?.focus();
  // A new entry starts in the first box: at the start, and again after wrong digits cleared the boxes.
  useEffect(() => {
    // Not while the boxes are disabled (an entry is being checked): the focus returns when they are enabled again.
    if (!disabled) focus(0);
  }, [resetKey, disabled]);

  const put = (index: number, raw: string) => {
    const typed = digitsOf(raw);
    if (!typed) return;
    // A paste or a fast type spreads over the boxes from this one.
    const next = (value.slice(0, index) + typed).slice(0, SIZE);
    onChange(next);
    focus(Math.min(next.length, SIZE - 1));
  };

  return (
    <div className="ac-boxes" role="group" aria-label="Six digits" data-bad={bad || undefined}>
      {Array.from({ length: SIZE }, (_, index) => (
        <input
          key={index}
          ref={(node) => { boxes.current[index] = node; }}
          className="ac-box"
          type="text"
          inputMode="numeric"
          pattern="[0-9]*"
          autoComplete="off"
          maxLength={SIZE}
          aria-label={`Digit ${index + 1} of ${SIZE}`}
          aria-invalid={bad || undefined}
          value={value[index] ?? ""}
          disabled={disabled}
          onChange={(event) => {
            if (event.target.value === "") onChange(value.slice(0, index) + value.slice(index + 1));
            else put(index, event.target.value);
          }}
          onPaste={(event) => {
            event.preventDefault();
            put(Math.min(index, value.length), event.clipboardData.getData("text"));
          }}
          onKeyDown={(event) => {
            if (event.key === "Backspace" && !(value[index] ?? "")) {
              event.preventDefault();
              if (index > 0) {
                onChange(value.slice(0, index - 1) + value.slice(index));
                focus(index - 1);
              }
            } else if (event.key === "ArrowLeft") focus(index - 1);
            else if (event.key === "ArrowRight") focus(index + 1);
          }}
          onFocus={(event) => event.currentTarget.select()}
        />
      ))}
    </div>
  );
}
