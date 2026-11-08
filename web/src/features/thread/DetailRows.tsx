import { ChevronDown, ChevronRight } from "lucide-react";
import type { ReactNode } from "react";

/** The still grey bar a row shows in the value position while its data loads. */
export function ValueSkeleton({ width }: { width: number }) {
  return <span className="sk" style={{ width }} aria-hidden />;
}

/**
 * One row of the thread details card: icon, label and a right-aligned value with an optional
 * quiet line under it. A row with an action is a button in the tab order; `extra` sits beside it.
 */
export function DRow({
  icon,
  label,
  value,
  sub,
  subLines,
  onClick,
  disabled,
  loading,
  waited,
  skeleton = 80,
  action,
  expanded,
  extra,
  title,
  ariaLabel,
  children,
}: {
  icon: ReactNode;
  label: string;
  value?: ReactNode;
  sub?: ReactNode;
  /** Several quiet lines under the value, such as one address each. */
  subLines?: ReactNode[];
  onClick?(): void;
  disabled?: boolean;
  /** Still waiting for data; nothing shows for the first 400 ms, then a still bar. */
  loading?: boolean;
  waited?: boolean;
  skeleton?: number;
  /** The shortcut or hint on an action row, which reads in mono. */
  action?: boolean;
  expanded?: boolean;
  extra?: ReactNode;
  title?: string;
  /** The row's name when its pieces read run together. */
  ariaLabel?: string;
  children?: ReactNode;
}) {
  const body = (
    <>
      {icon}
      <span className="dl">{label}</span>
      <span className="dv">
        {loading ? (waited ? <ValueSkeleton width={skeleton} /> : null) : <span className="row1">{value}</span>}
        {!loading && sub ? <small>{sub}</small> : null}
        {!loading && subLines ? subLines.map((line, at) => <small key={at}>{line}</small>) : null}
      </span>
      {expanded === undefined ? null : expanded ? <ChevronDown size={14} aria-hidden /> : <ChevronRight size={14} aria-hidden />}
    </>
  );
  return (
    <>
      <div className={`drow${action ? " act" : ""}`} aria-busy={loading || undefined}>
        {onClick ? (
          <button type="button" className="dmain" title={title} aria-label={ariaLabel} disabled={disabled} aria-expanded={expanded} onClick={onClick}>
            {body}
          </button>
        ) : (
          <div className="dmain" title={title}>
            {body}
          </div>
        )}
        {extra}
      </div>
      {children}
    </>
  );
}
