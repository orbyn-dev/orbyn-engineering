import { useRef } from "react";

/** A short list of exclusive choices shown side by side; arrow keys move the choice. */
export function Segmented<T extends string>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: { value: T; label: string; disabled?: boolean }[];
  onChange(value: T): void;
}) {
  const group = useRef<HTMLDivElement>(null);
  return (
    <div
      ref={group}
      className="segmented"
      role="radiogroup"
      aria-label={label}
      onKeyDown={(event) => {
        const step = event.key === "ArrowRight" || event.key === "ArrowDown" ? 1 : event.key === "ArrowLeft" || event.key === "ArrowUp" ? -1 : 0;
        if (!step) return;
        event.preventDefault();
        const at = options.findIndex((option) => option.value === value);
        // The next choice that is not disabled, in the direction asked.
        const next = Array.from({ length: options.length }, (_, n) => options[(at + step * (n + 1) + options.length * options.length) % options.length]).find((option) => option && !option.disabled);
        if (!next || next.value === value) return;
        onChange(next.value);
        requestAnimationFrame(() => group.current?.querySelector<HTMLElement>('[aria-checked="true"]')?.focus());
      }}
    >
      {options.map((option, at) => (
        <button
          key={option.value}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          disabled={option.disabled}
          tabIndex={option.value === value || (at === 0 && !options.some((entry) => entry.value === value)) ? 0 : -1}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}
