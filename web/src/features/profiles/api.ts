import { useEffect, useState } from "react";
import { ApiFailure, request } from "../../api/client";
import { useAsync } from "../../lib/useAsync";
import { PROFILE_LIMIT, type ApplyAnswer, type ApplyBody, type Profile, type ProfileCheck, type ProfileDraft, type ProfileList, type ProfilesApi } from "./contract";

const q = encodeURIComponent;
const isMissing = (error: unknown) => error instanceof ApiFailure && error.status === 404;
const isObject = (value: unknown): value is Record<string, unknown> => typeof value === "object" && value !== null && !Array.isArray(value);
const isProfileLike = (value: unknown): value is Record<string, unknown> => isObject(value) && typeof value.id === "string";

/**
 * The real client for the routes of docs/execution-profiles-and-library.md, Part 1. `GET /api/profiles` answers a bare array of
 * profiles (the limit of 50 is a constant, not in the answer); `404` is "this service has no profiles" and is answered as `null`
 * with no error, so nothing draws. Every write is Person-class (`personKey.ts`): `request` adds the key. A profile id in a path is encoded and
 * only ever one the list gave. Crew routes answer 404 with Crew Preview off: the Settings page is hidden then too.
 */
const http: ProfilesApi = {
  async list(signal) {
    try {
      const list = await request<unknown>("GET", "/api/profiles", undefined, signal);
      if (!Array.isArray(list)) return null;
      return { profiles: list.filter(isProfileLike) as unknown as Profile[], limit: PROFILE_LIMIT };
    } catch (error) {
      if (isMissing(error)) return null;
      throw error;
    }
  },
  check: (draft, signal) => request<ProfileCheck>("POST", "/api/profiles/check", draft, signal),
  create: (draft: ProfileDraft) => request<Profile>("POST", "/api/profiles", draft),
  update: (id, draft, seenRevision) => request<Profile>("PATCH", `/api/profiles/${q(id)}`, { ...draft, seen_revision: seenRevision }),
  async remove(id) {
    await request<unknown>("DELETE", `/api/profiles/${q(id)}`);
  },
  applyToThread: (threadId, body: ApplyBody) => request<ApplyAnswer>("PUT", `/api/threads/${q(threadId)}/profile`, body),
  applyToCrew: (crewId, body) => request<ApplyAnswer>("POST", "/api/crew/profile/apply", { crew_id: crewId, ...body }),
};

let current: ProfilesApi = http;
let version = 0;
/** Whether the service has the feature: read once per client, so the composer and Settings ask the service one time. */
let probe: Promise<boolean> | null = null;
const listeners = new Set<() => void>();

// Dev only: `#/preview/profiles` installs the mock so the screens can be seen before the routes exist. In a production build the flag
// is false, this branch is dead code and the mock is never emitted.
if (__ORBYN_PREVIEWS__ && typeof location !== "undefined" && location.hash.startsWith("#/preview/profiles")) {
  void import("../../../mock/profiles").then((module) => setProfilesApi(module.createProfilesMock({ delayMs: 300 })));
}

export const profilesApi = (): ProfilesApi => current;

/** Tests and the dev-only preview install the mock here; `null` puts the real client back. */
export function setProfilesApi(next: ProfilesApi | null): void {
  current = next ?? http;
  probe = null;
  version += 1;
  for (const listener of listeners) listener();
}

/** Changes when the client is swapped, so a screen reads again from the new one. */
export function useApiVersion(): number {
  const [seen, setSeen] = useState(version);
  useEffect(() => {
    const listener = () => setSeen(version);
    listeners.add(listener);
    listener();
    return () => { listeners.delete(listener); };
  }, []);
  return seen;
}

/** `true` once the service answered the list; `false` while asking, on `404`, and on any failure (the feature stays hidden, no error). */
export function useProfilesOffered(): boolean {
  const seen = useApiVersion();
  const [offered, setOffered] = useState(false);
  useEffect(() => {
    let live = true;
    probe ??= profilesApi().list().then((list) => list !== null, () => false);
    void probe.then((answer) => { if (live) setOffered(answer); });
    return () => { live = false; };
  }, [seen]);
  return offered;
}

/** The profile list. `list` is `null` while loading and when the service reports no profiles feature. The read is latest-wins (`useAsync`). */
export function useProfiles(): { list: ProfileList | null; error: ApiFailure | null; settled: boolean; reload(): void } {
  const seen = useApiVersion();
  const read = useAsync(`profiles:${seen}`, (signal) => profilesApi().list(signal));
  return {
    list: read.status === "ready" ? read.data : null,
    error: read.status === "error" ? read.error : null,
    settled: read.status !== "loading",
    reload: read.reload,
  };
}
