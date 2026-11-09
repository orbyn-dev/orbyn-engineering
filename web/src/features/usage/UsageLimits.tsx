import type { EngineLimits, LimitsOverview } from "../../api/types";

function resetsIn(iso: string): string {
  const left = new Date(iso).getTime() - Date.now();
  if (left <= 0) return "soon";
  const minutes = Math.round(left / 60_000);
  if (minutes < 60) return `in ${minutes}m`;
  const hours = Math.round(minutes / 60);
  return hours < 48 ? `in ${hours}h` : `in ${Math.round(hours / 24)}d`;
}

/** How long Limits may load before the page stops waiting for the engine. */
export const LIMITS_WAIT_MS = 8000;

function LimitRow({ label, used, resets }: { label: string; used: number; resets: string | null }) {
  const left = Math.max(0, Math.round(100 - used));
  return (
    <div className="limit-row" data-low={left < 15 || undefined}>
      <div className="limit-head">
        <span>{label}</span>
        <strong>{left}% left</strong>
      </div>
      <div
        className="limit-meter"
        role="meter"
        aria-label={label}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={left}
        aria-valuetext={`${left}% left`}
      >
        <span style={{ width: `${left}%` }} data-low={left < 15 || undefined} />
      </div>
      {resets ? <span className="limit-reset">Resets {resetsIn(resets)}</span> : null}
    </div>
  );
}

export function LimitsView({ data, nameOf }: { data: LimitsOverview; nameOf(engine: string): string }) {
  const empty = data.pooled.length === 0 && data.engines.length === 0;
  return (
    <div className="usage-body" data-limits>
      {/* The engine's own reason is for logs: the page says one plain thing, muted, as the usage-limits-unreported frame does. */}
      {data.unavailable.map((entry) => (
        <p key={entry.engine} className="usage-note" data-unreported>
          {nameOf(entry.engine)} doesn{"\u2019"}t report its limits.
        </p>
      ))}
      {empty ? <p className="usage-note">No engine reports limits.</p> : null}
      {data.pooled.length > 0 ? (
        <section className="usage-section">
          <h2>Pooled</h2>
          <div className="limit-grid">
            {data.pooled.map((window) => (
              <LimitRow
                key={window.label}
                label={window.label}
                used={window.used_percent}
                resets={window.resets_at}
              />
            ))}
          </div>
        </section>
      ) : null}
      {data.engines.map((engine: EngineLimits) => (
        <section key={engine.engine} className="usage-section">
          <h2>
            {nameOf(engine.engine)}
            {engine.plan ? <span className="usage-plan">{engine.plan}</span> : null}
          </h2>
          <div className="limit-grid">
            {engine.windows.map((window) => (
              <LimitRow
                key={window.id}
                label={window.label}
                used={window.used_percent}
                resets={window.resets_at}
              />
            ))}
          </div>
        </section>
      ))}
    </div>
  );
}
