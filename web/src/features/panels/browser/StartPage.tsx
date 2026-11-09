import { Square } from "lucide-react";
import { request } from "../../../api/client";
import type { BrowserProfile, LocalServer } from "../../../api/types";
import { toFailure, useAsync } from "../../../lib/useAsync";
import { useUi } from "../../../state/ui";
import { IconButton } from "../../../ui/controls";
import { q } from "./browserModel";

/** Reachable servers from this thread's processes, and pages opened before. */
export function StartPage({
  threadId,
  onOpen,
  profile,
  setProfile,
  isPrivate,
}: {
  threadId: string;
  onOpen(url: string): void;
  profile: string;
  setProfile(id: string): void;
  isPrivate: boolean;
}) {
  const profiles = useAsync("browser-profiles", (signal) =>
    request<BrowserProfile[]>("GET", "/api/browser-profiles", undefined, signal),
  );
  const servers = useAsync(`servers:${threadId}`, (signal) =>
    request<LocalServer[]>("GET", `/api/threads/${q(threadId)}/servers`, undefined, signal),
  );
  const recent = useUi((state) => state.recentUrls);
  const pushToast = useUi((state) => state.pushToast);
  const list = servers.data ?? [];
  return (
    <div className="browser-start">
      <h3>No preview yet</h3>
      <p>Type a URL above, or run a dev script. Browser-ready localhost servers will show up here automatically.</p>
      {isPrivate ? <p>This tab is private: its pages stay out of your history.</p> : null}
      {profiles.data && profiles.data.length > 0 ? (
        <label className="browser-profile">
          Profile
          <select aria-label="Profile" value={profile} onChange={(event) => setProfile(event.target.value)}>
            <option value="">None, ends with the tab</option>
            {profiles.data.map((entry) => (
              <option key={entry.id} value={entry.id} disabled={entry.in_use}>
                {entry.name}
              </option>
            ))}
          </select>
        </label>
      ) : null}
      {list.length > 0 ? (
        <ul aria-label="Local servers">
          {list.map((server) => (
            <li key={server.port}>
              <button type="button" onClick={() => onOpen(`http://localhost:${server.port}`)}>
                localhost:{server.port} <span>{server.process}</span>
              </button>
              {server.in_checkout ? (
                <IconButton
                  label={`Stop localhost:${server.port}`}
                  onClick={() => {
                    request<void>("POST", `/api/threads/${q(threadId)}/servers/${server.pid}/stop`).then(servers.reload, (failure: unknown) =>
                      pushToast(`Couldn't stop it. ${toFailure(failure).message}`),
                    );
                  }}
                >
                  <Square size={12} aria-hidden />
                </IconButton>
              ) : null}
            </li>
          ))}
        </ul>
      ) : null}
      {recent.length > 0 ? (
        <ul aria-label="Recent pages">
          {recent.map((url) => (
            <li key={url}>
              <button type="button" onClick={() => onOpen(url)}>
                {url}
              </button>
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}
