import type { ModelUsage, UsageReport } from "../../api/types";
import { cacheHit, costPerMillion, engineShares, formatPercent, formatUsd } from "./usageModel";

export function Segment<T extends string | number>({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: T;
  options: readonly { value: NoInfer<T>; label: string }[];
  onChange(value: NoInfer<T>): void;
}) {
  return (
    <div className="segment" role="radiogroup" aria-label={label}>
      {options.map((option) => (
        <button
          key={String(option.value)}
          type="button"
          role="radio"
          aria-checked={option.value === value}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export function Stat({ label, value, note }: { label: string; value: string; note?: string }) {
  return (
    <div className="usage-stat">
      <span>{label}</span>
      <strong>{value}</strong>
      {note ? <small>{note}</small> : null}
    </div>
  );
}

/** A model's daily tokens across the period, scaled to its own busiest day. */
export function Sparkline({ values, label }: { values: readonly number[]; label: string }) {
  const width = 72;
  const height = 18;
  const max = Math.max(1, ...values);
  const step = values.length > 1 ? width / (values.length - 1) : 0;
  const points = values.map((value, at) => `${(at * step).toFixed(1)},${(height - 1 - (value / max) * (height - 2)).toFixed(1)}`);
  return (
    <svg className="usage-spark" width={width} height={height} role="img" aria-label={label}>
      <polyline points={points.join(" ")} fill="none" stroke="currentColor" strokeWidth={1.25} />
    </svg>
  );
}

export const perMillion = (model: ModelUsage) => {
  const rate = costPerMillion(model);
  return rate === null ? "—" : `${formatUsd(rate)}/M`;
};

export const hitText = (model: ModelUsage) => {
  const hit = cacheHit(model);
  return hit === null ? "—" : formatPercent(hit);
};

/** Each provider's part of the period, in one bar. */
export function ShareBar({
  engines,
  metric,
  colorOf,
  nameOf,
}: {
  engines: Parameters<typeof engineShares>[0];
  metric: "cost" | "tokens";
  colorOf(engine: string): string;
  nameOf(engine: string): string;
}) {
  const shares = engineShares(engines, metric);
  if (shares.length === 0) return null;
  const label = metric === "cost" ? "Share of priced cost" : "Share of tokens";
  return (
    <div className="usage-sharebar" role="group" aria-label={label}>
      {shares.map((entry) => (
        <span
          key={entry.engine}
          title={`${nameOf(entry.engine)} ${formatPercent(entry.share)}`}
          aria-label={`${nameOf(entry.engine)} ${formatPercent(entry.share)}`}
          style={{ width: `${entry.share * 100}%`, background: colorOf(entry.engine) }}
        />
      ))}
    </div>
  );
}

/** What the last local scan read, so a short or empty period can be explained. */
export function ScanStatus({ report, nameOf }: { report: UsageReport; nameOf(engine: string): string }) {
  const at = new Date(report.scanned_at);
  const when = Number.isNaN(at.getTime()) ? report.scanned_at : at.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });
  const parts = report.engines.map((engine) => {
    if (engine.status === "no_history") return `${nameOf(engine.engine)}: no local history`;
    const skipped = engine.files_skipped > 0 ? `, ${engine.files_skipped} unreadable` : "";
    return `${nameOf(engine.engine)}: ${engine.files_scanned} ${engine.files_scanned === 1 ? "file" : "files"}${skipped}`;
  });
  return (
    <p className="usage-note usage-scan" aria-label="Scan status">
      Local history scanned at {when}. {parts.join(" · ")}
    </p>
  );
}
