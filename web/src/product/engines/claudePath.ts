import { useCallback, useEffect, useRef, useState } from "react";
import { queryPreferences, updatePreferences } from "../../api/preferences";
import type { ScopedPreferenceRow } from "../../generated/ScopedPreferenceRow";
import { toFailure } from "../../lib/useAsync";
import { useUi } from "../state/ui";

/** The two values the service accepts; anything else is read as the default. */
export type ClaudeEnginePath = "sdk" | "cli";

/** The one helper line under each Claude path, in the Providers list and the engine picker. */
export const CLAUDE_SDK_NOTE = "Runs through Orbyn’s engine host with the Claude Agent SDK, using your own Claude Code. Default.";
export const CLAUDE_CLI_NOTE = "Runs your Claude Code directly, without the engine host.";

/** The one catalog key the Claude path reads and writes. It is app-only: one choice for this computer. */
export const CLAUDE_ENGINE_PATH_KEY = "claude_engine_path";
export const CLAUDE_ENGINE_PATH_DEFAULT: ClaudeEnginePath = "sdk";

interface ReadState {
  version: string;
  row: ScopedPreferenceRow | null;
}

export interface ClaudePathSave {
  saved: boolean;
  error: string | null;
}

/**
 * The Claude path setting, read with the settings list's own scoped-preference client and written with the same
 * `membership_version` and per-row `if_match` compare-and-set. A failed write changes nothing here, so the screen
 * keeps the value it read; the caller shows the failure. `loaded` is false until the service answers with the row.
 */
export function useClaudeEnginePath(): {
  path: ClaudeEnginePath;
  loaded: boolean;
  readError: string | null;
  choose(next: ClaudeEnginePath): Promise<ClaudePathSave>;
} {
  const revision = useUi((state) => state.preferencesRevision);
  const [state, setState] = useState<ReadState | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const pending = useRef(false);

  useEffect(() => {
    const abort = new AbortController();
    queryPreferences({ scope: {}, keys: [CLAUDE_ENGINE_PATH_KEY] }, abort.signal).then(
      (result) => {
        if (abort.signal.aborted) return;
        setState({
          version: result.catalog.version,
          row: result.rows.find((row) => row.key === CLAUDE_ENGINE_PATH_KEY) ?? null,
        });
        setReadError(null);
      },
      (failure: unknown) => {
        if (abort.signal.aborted) return;
        setReadError(toFailure(failure).message);
      },
    );
    return () => abort.abort();
  }, [revision]);

  const path: ClaudeEnginePath = state?.row?.value === "cli" ? "cli" : CLAUDE_ENGINE_PATH_DEFAULT;

  const choose = useCallback(async (next: ClaudeEnginePath): Promise<ClaudePathSave> => {
    if (pending.current) return { saved: false, error: null };
    const row = state?.row;
    if (!row || !state) return { saved: false, error: "The Claude path setting is not available yet." };
    pending.current = true;
    try {
      const changed = await updatePreferences({
        scope: {},
        membership_version: state.version,
        if_match: { [row.key]: row.revision },
        edits: { [row.key]: { op: "set", value: next } },
      });
      useUi.getState().bumpPreferencesRevision();
      const changedRow = changed.rows.find((entry) => entry.key === CLAUDE_ENGINE_PATH_KEY);
      if (changedRow) setState({ version: changed.catalog.version, row: changedRow });
      return { saved: true, error: null };
    } catch (failure) {
      return { saved: false, error: toFailure(failure).message };
    } finally {
      pending.current = false;
    }
  }, [state]);

  return { path, loaded: state?.row != null, readError, choose };
}

/** True for the Claude family, including a named instance; no other engine has this choice. */
export function isClaudeEngine(id: string, instanceOf?: string | null): boolean {
  return (instanceOf ?? id) === "claude";
}
