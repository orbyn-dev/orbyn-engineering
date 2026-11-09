import { useMemo, useState } from "react";
import type { UsageReport } from "../../api/types";
import { UsageChart } from "./UsageChart";
import { formatPercent, formatUsd, modelTrend, TIER_LABELS, usageTotals, type UsageDays } from "./usageModel";
import { formatTokens } from "../../lib/format";
import { Segment, ShareBar, Sparkline, Stat, hitText, perMillion } from "./UsageParts";
import { ModelPriceDialog } from "./ModelPriceDialog";

const CHART_COLORS = ["var(--color-text)", "var(--color-textSoft)", "var(--color-muted)", "var(--color-faint)"];

export function UsageView({
  report,
  metric,
  days,
  only,
  nameOf,
  onSaved,
}: {
  report: UsageReport;
  metric: "cost" | "tokens";
  days: UsageDays;
  only: string | null;
  nameOf(engine: string): string;
  /** A price or alias changed, so the report does too. */
  onSaved(): void;
}) {
  const totals = useMemo(
    () => usageTotals(report.engines.filter((engine) => !only || engine.engine === only), days, new Date()),
    [report, days, only],
  );
  const [breakdown, setBreakdown] = useState<"model" | "time">("model");
  const [pricing, setPricing] = useState<string | null>(null);
  const colorOf = (engine: string) => {
    const at = report.engines.findIndex((entry) => entry.engine === engine);
    return CHART_COLORS[at % CHART_COLORS.length] ?? "var(--color-text)";
  };
  if (totals.engines.length === 0) {
    return (
      <div className="usage-body">
        <p className="usage-note">No usage in this period.</p>
      </div>
    );
  }
  const cost = (value: number | null) => (value === null ? "Unpriced" : formatUsd(value));
  const models = totals.engines.flatMap((engine) => engine.models).sort((a, b) => b.tokens - a.tokens);
  const chart = days > 1 && (metric === "tokens" || totals.cost !== null);
  const cachedShare = totals.cached + totals.input > 0 ? totals.cached / (totals.cached + totals.input) : null;
  const tokensTile = { label: "Processed tokens", value: formatTokens(totals.tokens) };
  const costTile = {
    label: "Estimated cost",
    value: cost(totals.cost),
    note: `API estimate${totals.unpriced && totals.cost !== null ? ", some usage is unpriced" : ""}`,
  };
  const dated = [...totals.dates].reverse().filter((date) => totals.engines.some((engine) => engine.daily.has(date)));
  return (
    <div className="usage-body">
      <section className="usage-tiles" aria-label="Totals">
        <Stat {...tokensTile} />
        <Stat {...costTile} />
        <Stat label="Cache hits" value={cachedShare === null ? "—" : formatPercent(cachedShare)} />
      </section>
      <p className="usage-note usage-detail">
        Output {formatTokens(totals.output)} · Cached input {formatTokens(totals.cached)} · Uncached input {formatTokens(totals.input)}
      </p>
      <section className="usage-cols">
        {chart ? (
          <div className="usage-plot">
            <h2>{metric === "cost" ? "Daily estimated cost" : "Daily processed tokens"}</h2>
            <UsageChart
              engines={totals.engines}
              dates={totals.dates}
              colorOf={colorOf}
              nameOf={nameOf}
              metric={metric}
            />
          </div>
        ) : null}
        <div className="usage-summary">
          <h2>Providers</h2>
          {totals.engines.length > 1 ? (
            <ShareBar engines={totals.engines} metric={metric} colorOf={colorOf} nameOf={nameOf} />
          ) : null}
          {totals.engines.map((engine) => {
            const share = totals.tokens > 0 ? engine.tokens / totals.tokens : 0;
            return (
              <div key={engine.engine} className="usage-engine">
                <div>
                  <span>
                    <span className="usage-dot" style={{ background: colorOf(engine.engine) }} />
                    {nameOf(engine.engine)}
                  </span>
                  <strong>{metric === "cost" ? cost(engine.cost) : formatTokens(engine.tokens)}</strong>
                </div>
                <span className="usage-share">
                  {metric === "cost"
                    ? `${formatTokens(engine.tokens)} tokens`
                    : `${formatPercent(share)} of tokens · ${cost(engine.cost)}`}
                </span>
              </div>
            );
          })}
        </div>
      </section>
      {metric === "cost" ? (
        <section className="usage-section" aria-label="Cost split">
          <h2>Cost by token type</h2>
          {totals.costSplit ? (
            <div className="usage-stats">
              <Stat label="Uncached input" value={formatUsd(totals.costSplit.input)} />
              <Stat label="Output" value={formatUsd(totals.costSplit.output)} />
              <Stat label="Cached input" value={formatUsd(totals.costSplit.cache_read)} />
              <Stat label="Cache write" value={formatUsd(totals.costSplit.cache_write)} />
            </div>
          ) : (
            <p className="usage-note">No model in this period has a price.</p>
          )}
          <h2>Cost by tier</h2>
          {totals.tiers.some((tier) => tier.tier !== "standard" && tier.unpricedTokens > 0) ? (
            <p className="usage-note" role="note">
              A price without a tier now covers the standard tier only. Set a price for each other tier from its model.
            </p>
          ) : null}
          <div className="usage-stats">
            {totals.tiers.map((tier) => (
              <Stat
                key={tier.tier}
                label={`${TIER_LABELS[tier.tier] ?? tier.tier} · ${formatTokens(tier.tokens)} tokens`}
                value={
                  tier.cost === null
                    ? "Unpriced"
                    : `${formatUsd(tier.cost)}${tier.unpricedTokens > 0 ? ` + ${formatTokens(tier.unpricedTokens)} unpriced` : ""}`
                }
              />
            ))}
          </div>
        </section>
      ) : null}
      <section className="usage-section">
        <div className="usage-section-head">
          <h2>Breakdown</h2>
          <Segment
            label="Usage breakdown"
            value={breakdown}
            options={[
              { value: "model", label: "Model" },
              { value: "time", label: "Day" },
            ]}
            onChange={setBreakdown}
          />
        </div>
        <div className="usage-table">
          {breakdown === "model" ? (
            <table>
              <thead>
                <tr>
                  <th>Model</th>
                  <th>Cost</th>
                  <th>Per million</th>
                  <th>Cache hit</th>
                  <th>Trend</th>
                  <th>Share</th>
                  <th>Tokens</th>
                </tr>
              </thead>
              <tbody>
                {models.map((model) => (
                  <tr key={`${model.engine}:${model.model}`}>
                    <td>
                      <button type="button" className="text-action" aria-label={`Set price for ${model.model}`} onClick={() => setPricing(model.model)}>
                        {model.model}
                      </button>
                    </td>
                    <td>{cost(model.estimated_cost)}</td>
                    <td>{perMillion(model)}</td>
                    <td>{hitText(model)}</td>
                    <td>
                      <Sparkline values={modelTrend(model, totals.dates)} label={`${model.model} daily tokens`} />
                    </td>
                    <td>{formatPercent(totals.tokens > 0 ? model.tokens / totals.tokens : 0)}</td>
                    <td>{formatTokens(model.tokens)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <table>
              <thead>
                <tr>
                  <th>Day</th>
                  {totals.engines.map((engine) => (
                    <th key={engine.engine}>{nameOf(engine.engine)}</th>
                  ))}
                  <th>Tokens</th>
                </tr>
              </thead>
              <tbody>
                {dated.map((date) => (
                  <tr key={date}>
                    <td>{date}</td>
                    {totals.engines.map((engine) => (
                      <td key={engine.engine}>{formatTokens(engine.daily.get(date) ?? 0)}</td>
                    ))}
                    <td>
                      {formatTokens(totals.engines.reduce((sum, engine) => sum + (engine.daily.get(date) ?? 0), 0))}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </section>
      {pricing ? <ModelPriceDialog model={pricing} onClose={() => setPricing(null)} onSaved={onSaved} /> : null}
    </div>
  );
}
