import { useEffect } from "react";
import { create } from "zustand";
import { api, type ApiFailure } from "../../api/client";
import type { SkillPackStatus } from "../../generated/SkillPackStatus";
import { toFailure } from "../../lib/useAsync";
import { useWorkspace } from "../../state/workspace";

interface SkillsStore {
  data: SkillPackStatus | null;
  error: ApiFailure | null;
  loading: boolean;
  /** Reads the library once at a time: a second ask while one is out waits for it. */
  load(): Promise<void>;
}

let inflight: Promise<void> | null = null;

export const useSkillsStore = create<SkillsStore>((set) => ({
  data: null,
  error: null,
  loading: false,
  load() {
    if (inflight) return inflight;
    set({ loading: true });
    inflight = api
      .skills()
      .then(
        (data) => set({ data, error: null, loading: false }),
        (failure: unknown) => set({ error: toFailure(failure), loading: false }),
      )
      .finally(() => {
        inflight = null;
      });
    return inflight;
  },
}));

/**
 * The skills library, read once for the sidebar and the page together. It is read again whenever the
 * library's own revision changes, such as after an update, so the count never goes stale.
 */
export function useSkills() {
  const revision = useWorkspace((state) => state.managed.data?.library?.revision);
  const load = useSkillsStore((state) => state.load);
  useEffect(() => {
    void load();
  }, [load, revision]);
  const { data, error, loading } = useSkillsStore();
  return { data, error, loading, reload: load };
}

/** Forgets what was read, for a test that starts from nothing. */
export function resetSkillsStore() {
  inflight = null;
  useSkillsStore.setState({ data: null, error: null, loading: false });
}
