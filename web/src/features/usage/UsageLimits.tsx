import { Loader2 } from "lucide-react";
import type { ReactNode } from "react";
import type { EngineLimits, EngineStatus, LimitsOverview } from "../../api/types";
import { LinkButton, Button } from "../../ui/controls";
import { routeHref } from "../../product/state/route";

/** How long Limits may load before the page stops waiting for the engine. */
export const LIMITS_WAIT_MS = 8000;

/** The one engine whose window Orbyn reads. Every other engine reads Not reported (usage-claude-windows). */
const WINDOW_ENGINE = "codex";

const DAYS = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const two = (n: number) => String(n).padStart(2, "0");

/** The window's name as the design words it: "5-hour window", "Weekly limit". */
function windowName(label: string, minutes: number | null | undefined): string {
  const m = minutes === null || minutes === undefined ? null : Number(minutes);
  if (m === 10080) return "Weekly limit";
  if (m && m >= 1440 && m % 1440 === 0) return `${m / 1440}-day window`;
  if (m && m >= 60 && m % 60 === 0) return `${m / 60}-hour window`;
  return label;
}

/** "Resets in 38 min", "Resets in 1 h 02 min", and past a day "Resets Mon 9 Nov"; the clock time sits on the right. */
export function resetText(iso: string | null, now = Date.now()): { sub: string; at: string } | null {
  if (!iso) return null;
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return null;
  const date = `${DAYS[at.getDay()]} ${at.getDate()} ${MONTHS[at.getMonth()]}`;
  const minutes = Math.max(0, Math.round((at.getTime() - now) / 60_000));
  if (minutes >= 24 * 60) return { sub: `Resets ${date}`, at: `Resets ${date}` };
  const left = minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} h ${two(minutes % 60)} min`;
  return { sub: `Resets in ${left}`, at: `Resets ${two(at.getHours())}:${two(at.getMinutes())}` };
}

/** The tile letter: the first letter, except where the design disambiguates. */
const TILE: Record<string, string> = { codex: "X", "cursor-agent": "Cu" };

function Tile({ id, name }: { id: string; name: string }) {
  return (
    <span className="limit-tile" aria-hidden>
      {TILE[id] ?? name.slice(0, 1).toUpperCase()}
    </span>
  );
}

function Status({ children }: { children: string }) {
  return (
    <span className="limit-status">
      <i aria-hidden />
      {children}
    </span>
  );
}

function NotReportedRow({ label, note }: { label: string; note?: string }) {
  return (
    <div className="limit-win limit-win-plain">
      <div>
        <b>{label}</b>
        {note ? <small>{note}</small> : null}
      </div>
      <Status>Not reported</Status>
    </div>
  );
}

/** A quota window: bar, percentage and reset time. Near the cap and at it stay in neutral ink (usage-claude-windows-near). */
function WindowRow({ name, used, resets }: { name: string; used: number; resets: string | null }) {
  const percent = Math.min(100, Math.max(0, Math.round(used)));
  const reset = resetText(resets);
  const state = percent >= 100 ? "Limit reached" : percent >= 90 ? "Near the limit" : null;
  return (
    <div className="limit-win" role="group" aria-label={name}>
      <div>
        <b>{name}</b>
        {reset ? <small>{reset.sub}</small> : null}
      </div>
      <div className="limit-used">
        <span
          className="limit-bar"
          role="meter"
          aria-label={name}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={percent}
          aria-valuetext={`${percent}% used`}
        >
          <i style={{ width: `${percent}%` }} />
        </span>
        <span className="limit-pct">{percent}% used</span>
      </div>
      <span className="limit-end">
        {reset ? <span className="limit-at">{reset.at}</span> : null}
        {state ? <Status>{state}</Status> : null}
      </span>
    </div>
  );
}

function ProviderRow({ id, name, note, tone, children }: { id: string; name: string; note: string; tone?: "failed"; children: ReactNode }) {
  return (
    <div className="limit-prow">
      <Tile id={id} name={name} />
      <span>
        <b>{name}</b>
        <small data-tone={tone}>{note}</small>
      </span>
      <span className="limit-pend">{children}</span>
    </div>
  );
}

interface LimitsViewProps {
  data: LimitsOverview;
  nameOf(engine: string): string;
  /** The installed engines, so an engine with no answer still gets its row. */
  engines?: readonly EngineStatus[] | null;
  /** Reads the engine statuses and the limits again. */
  onCheck?(): void;
  /** Asks one engine again. */
  onRetry?(engine: string): void;
}

/** The Limits tab: a group per engine that reports one, and a row for each provider that does not (usage-claude-windows*, usage-providers*). */
export function LimitsView({ data, nameOf, engines, onCheck, onRetry }: LimitsViewProps) {
  const known = new Map((engines ?? []).map((engine) => [engine.id, engine]));
  const reported = new Map<string, EngineLimits>(data.engines.map((entry) => [entry.engine, entry]));
  const failed = new Set(data.unavailable.map((entry) => entry.engine));
  const present = (id: string) => known.get(id)?.installed === true || reported.has(id) || failed.has(id);

  const claude = present("claude");
  const codex = present(WINDOW_ENGINE);
  const codexWindows = reported.get(WINDOW_ENGINE)?.windows ?? [];
  // Signed out wins over any window in the answer: the service keeps a read per engine, so it can outlive a sign-out.
  const signedOut = codex && known.get(WINDOW_ENGINE)?.auth === "logged_out";
  const codexFailed = codex && !signedOut && failed.has(WINDOW_ENGINE);
  // Claude Code beside a calm Codex gets the short line; alone, or beside a window at its cap, the full one.
  const nearCap = codexWindows.some((entry) => entry.used_percent >= 90);
  const claudeNote = codex && !codexFailed && !nearCap
    ? `${nameOf("claude")} doesn\u2019t report one.`
    : `${nameOf("claude")} doesn\u2019t report one. Usage and cost are on the Usage tab.`;
  const others = [
    ...new Set([
      ...(engines ?? []).filter((engine) => engine.installed).map((engine) => engine.id),
      ...data.engines.map((entry) => entry.engine),
      ...data.unavailable.map((entry) => entry.engine),
    ]),
  ].filter((id) => id !== "claude" && id !== WINDOW_ENGINE);
  const listed = others.length > 0 || codexFailed;
  const empty = !claude && !codex && !listed;
  const plan = (id: string) => reported.get(id)?.plan;
  const title = (id: string) => (plan(id) ? `${nameOf(id)} · ${plan(id)}` : nameOf(id));

  return (
    <div className="usage-body limits" data-limits>
      {empty ? <p className="usage-note">No engine reports limits.</p> : null}
      {claude ? (
        <section className="limit-group" aria-label={title("claude")}>
          <h2>{title("claude")}</h2>
          <NotReportedRow label="Subscription window" note={claudeNote} />
        </section>
      ) : null}
      {codex && !codexFailed ? (
        <section className="limit-group" aria-label={title(WINDOW_ENGINE)}>
          <h2>{title(WINDOW_ENGINE)}</h2>
          {signedOut ? (
            <div className="limit-win limit-win-plain">
              <div>
                <b>Weekly limit</b>
                <small>{nameOf(WINDOW_ENGINE)} is signed out. Sign in with the CLI, then check again.</small>
              </div>
              <span className="limit-end">
                <LinkButton small href={routeHref({ name: "settings", section: "providers" })}>
                  Open Providers
                </LinkButton>
                {onCheck ? (
                  <Button small onClick={onCheck}>
                    Check again
                  </Button>
                ) : null}
              </span>
            </div>
          ) : codexWindows.length > 0 ? (
            codexWindows.map((entry) => (
              <WindowRow key={entry.id} name={windowName(entry.label, entry.window_minutes)} used={entry.used_percent} resets={entry.resets_at} />
            ))
          ) : (
            <NotReportedRow label="Subscription window" />
          )}
        </section>
      ) : null}
      {listed ? (
        <section className="limit-group" aria-label="Reported by each engine">
          <h2>Reported by each engine</h2>
          <p className="limit-intro">
            Orbyn reads a limit only through the engine{"’"}s own CLI or a supported session protocol. It never reads token files or calls a
            provider{"’"}s quota endpoint itself, so a provider with no such route says so.
          </p>
          {codexFailed ? (
            <ProviderRow id={WINDOW_ENGINE} name={nameOf(WINDOW_ENGINE)} tone="failed" note={"Couldn\u2019t read the limit."}>
              {onRetry ? (
                <Button small onClick={() => onRetry(WINDOW_ENGINE)}>
                  Try again
                </Button>
              ) : null}
            </ProviderRow>
          ) : null}
          {others.map((id) => (
            <ProviderRow key={id} id={id} name={nameOf(id)} note="No supported route yet">
              <Status>Not reported</Status>
            </ProviderRow>
          ))}
        </section>
      ) : null}
    </div>
  );
}

/** Limits while an engine is asked (usage-claude-windows-loading): the rows that need no answer stay, and Codex shows a quiet placeholder. */
export function LimitsAsking({ nameOf, engines }: { nameOf(engine: string): string; engines: readonly EngineStatus[] }) {
  const installed = (id: string) => engines.some((engine) => engine.id === id && engine.installed);
  return (
    <div className="usage-body limits" data-limits role="status">
      {installed("claude") ? (
        <section className="limit-group" aria-label={nameOf("claude")}>
          <h2>{nameOf("claude")}</h2>
          <NotReportedRow label="Subscription window" note={`${nameOf("claude")} doesn’t report one. Usage and cost are on the Usage tab.`} />
        </section>
      ) : null}
      {installed(WINDOW_ENGINE) ? (
      <section className="limit-group" aria-label={nameOf(WINDOW_ENGINE)}>
        <h2>{nameOf(WINDOW_ENGINE)}</h2>
        <p className="limit-intro">Asking {nameOf(WINDOW_ENGINE)}{"…"}</p>
        <div className="limit-win" aria-hidden>
          <div>
            <b>Weekly limit</b>
          </div>
          <div className="limit-used">
            <span className="limit-bar">
              <i style={{ width: 0 }} />
            </span>
          </div>
          <span className="limit-end">
            <Loader2 size={11} className="spin" aria-label="Loading" />
          </span>
        </div>
      </section>
      ) : null}
    </div>
  );
}
