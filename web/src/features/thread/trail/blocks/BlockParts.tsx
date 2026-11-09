import { Check, Copy } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

/** The copy button; it says Copied for a moment. */
export function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(timer.current), []);
  const copy = () => {
    void navigator.clipboard?.writeText(value).then(() => {
      setCopied(true);
      clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 1500);
    });
  };
  return (
    <>
      {copied ? (
        <span className="ex" data-find-skip role="status">
          Copied
        </span>
      ) : null}
      <button type="button" className="icb" aria-label={copied ? "Copied" : label} title={label} onClick={copy}>
        {copied ? <Check size={14} aria-hidden /> : <Copy size={14} aria-hidden />}
      </button>
    </>
  );
}

/** A bar under capped content: the control on the left, a quiet note on the right. */
export function MoreBar({ icon, label, note, pressed, onClick }: { icon: ReactNode; label: string; note: string; pressed?: boolean; onClick(): void }) {
  return (
    <button type="button" className="mo" aria-pressed={pressed} onClick={onClick}>
      {icon}
      {label}
      <span className="sp" />
      <span className="mt">{note}</span>
    </button>
  );
}

export function Kvs({ rows }: { rows: [string, string][] }) {
  return (
    <div className="kvs">
      {rows.flatMap(([key, value], at) => [
        <b key={`k${at}`} data-find-skip>
          {key}
        </b>,
        <span key={`v${at}`}>{value}</span>,
      ])}
    </div>
  );
}

/** A header line inside a block: an icon or `$`, what it is, and a quiet note. */
export function BlockHead({ lead, children, note, tools, counted }: { lead: ReactNode; children: ReactNode; note?: string | null; tools?: ReactNode; counted?: boolean }) {
  return (
    <div className="cm" data-find-skip={counted ? undefined : true}>
      <span className="pr" aria-hidden>
        {lead}
      </span>
      <span className="cx">{children}</span>
      {note ? <span className="ex">{note}</span> : null}
      {tools}
    </div>
  );
}

export function FooterText({ children }: { children: ReactNode }) {
  return (
    <span className="tfm" data-find-skip>
      {children}
    </span>
  );
}

export const Dot = () => <span className="dot"> · </span>;
