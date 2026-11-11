import { ChevronDown, RefreshCw } from "lucide-react";
import "./usage.css";
import { useEffect, useId, useRef, useState } from "react";
import { tabKeys } from "../../ui/tabKeys";
import { api } from "./api";
import { useCommand } from "../../product/shell/shortcuts";
import { useWaited } from "../../lib/motion";
import { readJson, writeJson } from "../../lib/storage";
import { under } from "../../lib/dataRefresh";
import { toFailure, useAsync } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { Button, IconButton } from "../../ui/controls";
import { Menu } from "../../ui/Menu";
import { Presence } from "../../ui/Presence";
import { PageBar } from "../../product/ui/PageBar";
import { ErrorState, Loading } from "../../product/ui/states";
import { useUsageReport } from "./useUsageReport";
import { USAGE_PERIODS, type UsageDays, type UsageMetric } from "./usageModel";
import { UsageView } from "./UsageView";
import { ScanStatus, Segment } from "./UsageParts";
import { LIMITS_WAIT_MS, LimitsView } from "./UsageLimits";

const PREFS_KEY = "orbyn.usage.v1";

/** Set once a usage report has loaded, so only the first visit says the history is being read. */
const SEEN_KEY = "orbyn.usage.seen.v1";

const METRICS: { id: "cost" | "tokens"; label: string }[] = [
  { id: "cost", label: "Cost" },
  { id: "tokens", label: "Tokens" },
];

/** What agents used, by day, provider and model, and the limits that matter most. */
export function UsagePage() {
  const saved = readJson<{ days?: UsageDays; metric?: UsageMetric }>(PREFS_KEY);
  const [days, setDays] = useState<UsageDays>(saved?.days ?? 30);
  const [metric, setMetric] = useState<UsageMetric>(saved?.metric ?? "tokens");
  const [only, setOnly] = useState<string | null>(null);
  const tabsId = useId();
  const lastMetric = useRef<"cost" | "tokens">(saved?.metric && saved.metric !== "limits" ? saved.metric : "tokens");
  const [menu, setMenu] = useState<{ x: number; y: number } | null>(null);
  const [refreshing, setRefreshing] = useState(false);
  const engines = useWorkspace((state) => state.engines.data);
  const pushToast = useUi((state) => state.pushToast);
  const nameOf = (id: string) => engines?.find((engine) => engine.id === id)?.name ?? id;

  const usage = useUsageReport(days);
  const [firstVisit] = useState(() => readJson<boolean>(SEEN_KEY) !== true);
  useEffect(() => {
    if (usage.data) writeJson(SEEN_KEY, true);
  }, [usage.data]);
  // A load that finishes inside 400 ms shows the report with no motion. A longer one shows
  // the static skeleton first, and the report then fades in over it.
  const loadingFirst = usage.pending && !usage.data;
  const skeletonShown = useWaited(loadingFirst);
  const [revealed, setRevealed] = useState(false);
  useEffect(() => {
    if (loadingFirst) setRevealed(skeletonShown);
  }, [loadingFirst, skeletonShown]);
  // A refresh dims the figures only if it outlasts the same wait.
  const dimmed = useWaited(usage.pending && usage.data !== null);
  const limits = useAsync(`limits:${metric === "limits"}`, (signal) =>
    metric === "limits" ? api.limits(false, signal) : Promise.resolve(null),
  { keep: true, refreshOn: metric === "limits" ? under("limits") : undefined },
);

  // An engine that never answers must not leave the tab loading for good.
  const limitsChecking = useWaited(metric === "limits" && limits.status === "loading");
  const limitsSlow = useWaited(metric === "limits" && limits.status === "loading", LIMITS_WAIT_MS);

  useEffect(() => {
    if (metric !== "limits") lastMetric.current = metric;
    writeJson(PREFS_KEY, { days, metric });
  }, [days, metric]);

  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      if (event.target instanceof HTMLElement && event.target.closest("input, textarea, select, [contenteditable='true']")) return;
      const next = { c: "cost", t: "tokens", l: "limits" }[event.key.toLowerCase()] as UsageMetric | undefined;
      if (next) setMetric(next);
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, []);

  useCommand("usage.period.day", () => setDays(1));
  useCommand("usage.period.week", () => setDays(7));
  useCommand("usage.period.month", () => setDays(30));
  useCommand("usage.period.quarter", () => setDays(90));

  const refresh = async () => {
    if (metric !== "limits") {
      await usage.refresh();
      return;
    }
    setRefreshing(true);
    try {
      await api.limits(true);
      void limits.reload();
    } catch (failure) {
      pushToast(`Couldn't refresh. ${toFailure(failure).message}`);
    } finally {
      setRefreshing(false);
    }
  };

  const providers = usage.data?.engines ?? [];
  const pending = metric === "limits" ? refreshing : usage.pending;

  return (
    <section className="usage-page" aria-label="Usage">
      <PageBar
        actions={
          <IconButton
            label={metric === "limits" ? "Refresh limits" : "Refresh usage"}
            disabled={pending}
            onClick={() => void refresh()}
          >
            <RefreshCw size={14} className={pending ? "spin" : undefined} aria-hidden />
          </IconButton>
        }
      >
        <h1 tabIndex={-1} data-page-title>
          Usage
        </h1>
      </PageBar>
      <div
        className="usage-tabs"
        role="tablist"
        aria-label="Usage view"
        onKeyDown={(event) => tabKeys(event, (at) => setMetric(at === 1 ? "limits" : lastMetric.current))}
      >
        {(["usage", "limits"] as const).map((tab) => (
          <button
            key={tab}
            type="button"
            role="tab"
            id={`${tabsId}-${tab}`}
            aria-controls={`${tabsId}-panel`}
            aria-selected={(metric === "limits") === (tab === "limits")}
            tabIndex={(metric === "limits") === (tab === "limits") ? 0 : -1}
            onClick={() => setMetric(tab === "limits" ? "limits" : lastMetric.current)}
          >
            {tab === "limits" ? "Limits" : "Usage"}
          </button>
        ))}
      </div>
      {metric === "limits" ? null : (
        <div className="usage-filters">
          <Segment
            label="Usage period"
            value={days}
            options={USAGE_PERIODS.map((period) => ({ value: period.days, label: period.label }))}
            onChange={setDays}
          />
          <button
            type="button"
            className="usage-picker"
            aria-haspopup="menu"
            onClick={(event) => {
              const rect = event.currentTarget.getBoundingClientRect();
              setMenu({ x: rect.left, y: rect.bottom + 4 });
            }}
          >
            {only ? nameOf(only) : "All providers"}
            <ChevronDown size={14} aria-hidden />
          </button>
          <Segment
            label="Usage metric"
            value={metric}
            options={METRICS.map((entry) => ({ value: entry.id, label: entry.label }))}
            onChange={setMetric}
          />
        </div>
      )}
      <div className="usage-scroll" role="tabpanel" id={`${tabsId}-panel`} aria-labelledby={`${tabsId}-${metric === "limits" ? "limits" : "usage"}`}>
        {metric === "limits" ? (
          limits.status === "error" ? (
            <ErrorState title="Couldn't load limits" failure={limits.error} onRetry={limits.reload} />
          ) : limits.status === "ready" && limits.data ? (
            <LimitsView data={limits.data} nameOf={nameOf} />
          ) : limitsSlow ? (
            <div className="usage-body">
              <p className="usage-note" role="status">
                Limits didn{"\u2019"}t answer in time.{" "}
                <Button small onClick={limits.reload}>Try again</Button>
              </p>
            </div>
          ) : limitsChecking ? (
            // Limits can take a few seconds: after the 400 ms wait a line says so (usage-first-load).
            <div className="usage-body usage-first" role="status">
              <p>Checking your limits…</p>
            </div>
          ) : (
            <div className="usage-body">
              <Loading label="Loading limits" rows={3} />
            </div>
          )
        ) : (
          <div className="usage-history" data-refreshing={dimmed || undefined}>
            {usage.pending && usage.data ? <p className="usage-refresh-status" role="status">Refreshing usage…</p> : null}
            {usage.error ? (
              <ErrorState
                title={usage.data ? "Couldn't refresh usage" : "Couldn't load usage"}
                failure={usage.error}
                onRetry={usage.data ? usage.refresh : usage.reload}
              />
            ) : null}
            <div aria-busy={usage.pending}>
              {usage.data ? (
                <div className="usage-data" data-reveal={revealed || undefined}>
                  <UsageView report={usage.data} metric={metric} days={days} only={only} nameOf={nameOf} onSaved={() => void usage.reload()} />
                  <ScanStatus report={usage.data} nameOf={nameOf} />
                </div>
              ) : usage.pending && firstVisit && skeletonShown ? (
                // The first scan reads the whole history, which takes a while and happens once (usage-first-load).
                <div className="usage-body usage-first" role="status">
                  <p>Reading your history…</p>
                  <p className="usage-note">This happens once.</p>
                </div>
              ) : usage.pending ? (
                <div className="usage-body"><Loading label="Loading usage" rows={4} /></div>
              ) : null}
            </div>
          </div>
        )}
      </div>
      <Presence>{menu ? (
        <Menu
          {...menu}
          label="Providers"
          onClose={() => setMenu(null)}
          items={[
            { label: "All providers", selected: only === null, onSelect: () => setOnly(null) },
            ...providers.map((provider) => ({
              label: nameOf(provider.engine),
              selected: only === provider.engine,
              onSelect: () => setOnly(provider.engine),
            })),
          ]}
        />
      ) : null}</Presence>
    </section>
  );
}
