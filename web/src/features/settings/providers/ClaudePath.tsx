import { Info } from "lucide-react";
import { useState, type ReactNode } from "react";
import {
  cantStartLead, engineHostApi, mismatchLine, plain, useAppUpdateOrbyn, useEngineHosts, type HostRow,
} from "../../enginehost";
import { toFailure } from "../../../lib/useAsync";
import { useClaudeEnginePath, type ClaudeEnginePath } from "../../../product/engines/claudePath";
import { Button } from "../../../ui/controls";
import { SettingRow } from "../rows/SettingRow";
import { SettingsBanner } from "../rows/SettingsBanner";
import { HostChip } from "../engineHost/HostParts";

export const CLAUDE_PATH_HELP = "SDK runs Claude through Orbyn’s engine host, using your own Claude Code. CLI runs your Claude Code directly. Both paths use the same transcript files.";

function PathChoice({ value, path, label, defaulted, disabled, choose }: {
  value: ClaudeEnginePath;
  path: ClaudeEnginePath;
  label: string;
  defaulted?: boolean;
  disabled: boolean;
  choose(value: ClaudeEnginePath): void;
}) {
  const selected = value === path;
  return (
    <div
      role="radio"
      aria-checked={selected}
      aria-disabled={disabled || undefined}
      tabIndex={disabled ? -1 : selected ? 0 : -1}
      data-value={value}
      className="setting-line claude-path-choice"
      onClick={() => { if (!disabled) choose(value); }}
      onKeyDown={(event) => {
        if (event.key !== " " && event.key !== "Enter") return;
        event.preventDefault();
        if (!disabled) choose(value);
      }}
    >
      <span className={selected ? "set-radio on" : "set-radio"} aria-hidden />
      <span className="claude-path-label">
        <span className="setting-line-title">{label}</span>
        {defaulted ? <span className="set-pill dot">Default</span> : null}
      </span>
    </div>
  );
}

/**
 * The Claude path, under Location on the Claude Code page. It is one app-only preference read at the start of every run,
 * so a choice applies to the next message with no restart and no dialog. The Engine host row is the SDK path's state; with
 * the CLI chosen it reads Not used.
 */
export function ClaudePathSettings() {
  const { path, loaded, readError, choose } = useClaudeEnginePath();
  const { hosts } = useEngineHosts();
  const host = hosts?.find((row) => row.id === "claude") ?? null;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const pick = async (next: ClaudeEnginePath) => {
    if (busy || !loaded || next === path) return;
    setBusy(true);
    setError(null);
    const result = await choose(next);
    if (!result.saved && result.error) setError(result.error);
    setBusy(false);
  };
  const disabled = !loaded || busy;
  return (
    <>
      <p className="set-grp">Claude path</p>
      <p className="pa-cap claude-path-help" id="claude-path-help">{CLAUDE_PATH_HELP}</p>
      <fieldset className="preference-control" disabled={disabled} aria-busy={busy || undefined}>
        <div
          className="claude-path-choices"
          role="radiogroup"
          aria-label="Claude path"
          aria-describedby="claude-path-help"
          onKeyDown={(event) => {
            const step = event.key === "ArrowDown" || event.key === "ArrowRight" ? 1 : event.key === "ArrowUp" || event.key === "ArrowLeft" ? -1 : 0;
            if (!step) return;
            event.preventDefault();
            const choices = Array.from(event.currentTarget.querySelectorAll<HTMLElement>('[role="radio"]'));
            const at = choices.findIndex((choice) => choice.getAttribute("aria-checked") === "true");
            const next = choices[(at + step + choices.length) % choices.length];
            if (!next) return;
            next.focus();
            const value = next.dataset.value;
            if (value === "sdk" || value === "cli") void pick(value);
          }}
        >
          <PathChoice value="sdk" path={path} label="Claude (SDK)" defaulted disabled={disabled} choose={(value) => void pick(value)} />
          <PathChoice value="cli" path={path} label="Claude (CLI)" disabled={disabled} choose={(value) => void pick(value)} />
        </div>
      </fieldset>
      {path === "cli" ? <p className="setting-line-desc claude-path-next" role="status">Next message uses Claude (CLI).</p> : null}
      {path === "cli" || host ? (
        <div className="claude-path-host">
          <SettingRow title="Engine host" description="Used by Claude (SDK)">
            <HostChip state={path === "cli" ? "not_used" : host!.state} />
          </SettingRow>
        </div>
      ) : null}
      {error ? <p className="field-error" role="alert">Nothing changed. {error}</p> : null}
      {readError ? <p className="field-error" role="alert">Couldn’t read the Claude path. {readError}</p> : null}
    </>
  );
}

/** The states whose host stops the SDK path; one that is only idle or starting says nothing here. */
const BANNER_STATES = new Set<HostRow["state"]>(["crashed", "restarting", "cant_start", "version_mismatch"]);

/** The service's sentence for a host that stops the SDK path; the built host lines speak when an older service sends no detail. */
function hostBannerText(host: HostRow): string | null {
  if (!BANNER_STATES.has(host.state)) return null;
  if (host.detail) return host.detail;
  if (host.state === "crashed" || host.state === "restarting") return "The engine host stopped and is restarting.";
  if (host.state === "version_mismatch") return mismatchLine(host);
  return `${cantStartLead(host)}${host.last_error ? ` Last error: ${plain(host.last_error.message, 200).replace(/\.$/, "")}.` : ""}`;
}

/**
 * The service's own sentence for a host that stops the SDK path, with the one action that keeps the person moving
 * (Use Claude (CLI)); Try again only where the host gave up, Update Orbyn only when an update is ready. Nothing switches
 * by itself and the choice stays on the SDK until the person changes it.
 */
export function ClaudePathBanner() {
  const { path, loaded, choose } = useClaudeEnginePath();
  const { hosts, reload } = useEngineHosts();
  const update = useAppUpdateOrbyn();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ text: string; prefix: string } | null>(null);
  const host = hosts?.find((row) => row.id === "claude") ?? null;
  const line = host ? hostBannerText(host) : null;
  if (!loaded || path !== "sdk" || !host || !line) return null;
  const waiting = host.state === "restarting" || host.state === "crashed";

  const run = async (work: () => Promise<void>, prefix: string) => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await work();
    } catch (failure) {
      setError({ text: toFailure(failure).message, prefix });
    } finally {
      setBusy(false);
    }
  };
  const switchToCli = () => run(async () => {
    const result = await choose("cli");
    if (!result.saved && result.error) throw new Error(result.error);
  }, "Nothing changed.");
  const retry = () => run(async () => {
    await engineHostApi().retry(host.id);
    reload();
  }, "That didn’t work.");
  const updateApp = () => run(async () => { if (update) await update(); }, "That didn’t work.");

  const actions: ReactNode[] = [];
  if (host.state === "version_mismatch" && update) {
    actions.push(<Button key="update" small busy={busy} onClick={() => void updateApp()}>Update Orbyn</Button>);
  }
  actions.push(<Button key="cli" small busy={busy} onClick={() => void switchToCli()}>Use Claude (CLI)</Button>);
  if (host.state === "cant_start" && host.can_retry) {
    actions.push(<Button key="retry" small busy={busy} onClick={() => void retry()}>Try again</Button>);
  }

  return (
    <>
      <SettingsBanner tone={waiting ? "info" : "err"} icon={waiting ? <Info size={16} aria-hidden /> : undefined} actions={<>{actions}</>}>
        {line}
      </SettingsBanner>
      {error ? <p className="field-error" role="alert">{error.prefix} {error.text}</p> : null}
    </>
  );
}
