import { Check, Search } from "lucide-react";
import { type ReactNode } from "react";
import { deriveProjectIdentity } from "../../../product/projects/projectIdentity";
import { type CheckKey, type PullState, type ReviewKey } from "../model/pullModel";

export type Kind = "project" | "filters" | "sort";

export const toggle = <T,>(list: readonly T[], value: T): T[] => (list.includes(value) ? list.filter((one) => one !== value) : [...list, value]);

export const STATE_OPTIONS: { value: PullState; label: string }[] = [
  { value: "open", label: "Open" },
  { value: "closed", label: "Closed" },
  { value: "merged", label: "Merged" },
];

export const REVIEW_OPTIONS: { value: ReviewKey; label: string }[] = [
  { value: "approved", label: "Approved" },
  { value: "changes_requested", label: "Changes requested" },
];

export const CHECK_OPTIONS: { value: CheckKey; label: string }[] = [
  { value: "passing", label: "Passing" },
  { value: "failing", label: "Failing" },
  { value: "running", label: "Running" },
];

export function Tick({ on, label, count, wait, lead, onClick }: { on: boolean; label: string; count?: number; wait?: boolean; lead?: ReactNode; onClick(): void }) {
  return (
    <button type="button" role="checkbox" aria-checked={on} className="pulls-mi" onClick={onClick}>
      <span className="pulls-cbx" aria-hidden>
        {on ? wait ? <i className="pulls-wait spin" /> : <Check size={10} strokeWidth={3} /> : null}
      </span>
      {lead}
      {label}
      {count !== undefined ? <span className="pulls-k">{count}</span> : null}
    </button>
  );
}

/** A person's round mark, in the colour their name gives. */
export function Avatar({ name }: { name: string }) {
  const identity = deriveProjectIdentity(name);
  return (
    <span className={`pulls-av project-color-${identity.color}`} aria-hidden>
      {identity.monogram}
    </span>
  );
}

/** The typed text in ink, the rest of the name muted. */
export function Matched({ name, find }: { name: string; find: string }) {
  const at = find ? name.toLowerCase().indexOf(find.toLowerCase()) : -1;
  if (at < 0) return <>{name}</>;
  return (
    <>
      {name.slice(0, at)}
      <span className="pulls-ink">{name.slice(at, at + find.length)}</span>
      {name.slice(at + find.length)}
    </>
  );
}

export function SearchBox({
  placeholder,
  value,
  onChange,
  label,
  inputProps,
}: {
  placeholder: string;
  value: string;
  onChange(next: string): void;
  label: string;
  inputProps?: React.InputHTMLAttributes<HTMLInputElement>;
}) {
  return (
    <div className="pulls-fld pulls-fld-menu">
      <div className="pulls-inp">
        <Search size={14} aria-hidden />
        <input aria-label={label} placeholder={placeholder} value={value} onChange={(event) => onChange(event.target.value)} {...inputProps} />
      </div>
    </div>
  );
}
