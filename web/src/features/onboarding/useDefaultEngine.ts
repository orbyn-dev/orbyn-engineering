import { useCallback, useEffect, useState } from "react";
import { queryPreferences, updatePreferences } from "./api";
import { toFailure } from "../../lib/useAsync";
import { useUi } from "../../product/state/ui";

/** The model default is one tuple: the engine, its model, effort and options move together. */
const KEYS = ["default_engine", "default_model", "default_effort", "default_model_options"] as const;

interface Row { key: string; value: unknown; revision: string }

/**
 * The engine a new thread starts on, read and saved the way Settings does. Choosing an engine here sets it with the engine's
 * automatic model, effort and options; a model picked in Settings is not kept. One call, one write.
 */
export function useDefaultEngine() {
  const [state, setState] = useState<{ engine: string | null; version: string; rows: Row[] } | null>(null);
  const revision = useUi((ui) => ui.preferencesRevision);
  useEffect(() => {
    const abort = new AbortController();
    queryPreferences({ scope: {}, keys: [...KEYS] }, abort.signal).then(
      (result) => {
        if (abort.signal.aborted) return;
        const rows = (result.rows ?? []) as Row[];
        const engine = rows.find((row) => row.key === "default_engine")?.value;
        setState({ engine: typeof engine === "string" && engine !== "" ? engine : null, version: result.catalog.version, rows });
      },
      () => undefined,
    );
    return () => abort.abort();
  }, [revision]);
  const choose = useCallback(
    async (engine: string) => {
      if (!state || KEYS.some((key) => !state.rows.some((row) => row.key === key))) throw new Error("Defaults are unavailable.");
      const if_match = Object.fromEntries(KEYS.map((key) => [key, state.rows.find((row) => row.key === key)!.revision]));
      try {
        await updatePreferences({
          scope: {},
          membership_version: state.version,
          if_match,
          edits: {
            default_engine: { op: "set", value: engine },
            default_model: { op: "set", value: "" },
            default_effort: { op: "set", value: "" },
            default_model_options: { op: "set", value: {} },
          },
        });
      } catch (failure) {
        throw new Error(toFailure(failure).message, { cause: failure });
      }
      useUi.getState().bumpPreferencesRevision();
    },
    [state],
  );
  return { engine: state?.engine ?? null, loaded: state !== null, choose };
}
