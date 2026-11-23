import { ArrowUp, Folder, GitBranch } from "lucide-react";
import { useState } from "react";
import { api } from "../../api/client";
import { parentPath } from "../../lib/fsPath";
import { useAsync } from "../../lib/useAsync";
import { Button } from "../../ui/controls";
import { Empty, ErrorState, Loading } from "./states";

/**
 * The folder above `path`, read from the text alone: used when the read failed and the service gave no `parent`. Answers `null`
 * at a root (`/`, `C:\\`) so the service picks its own start.
 */
export function parentOfPath(path: string): string | null {
  const trimmed = path.replace(/[\\/]+$/, "");
  const cut = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (cut < 0) return null;
  if (cut === 0) return trimmed.length > 1 ? "/" : null;
  const head = trimmed.slice(0, cut);
  return /^[A-Za-z]:$/.test(head) ? `${head}\\` : head;
}

export function FolderBrowser({
  actionLabel,
  busy,
  onChoose,
}: {
  actionLabel: string;
  busy?: boolean;
  onChoose(path: string): void;
}) {
  const [path, setPath] = useState<string | null>(null);
  const listing = useAsync(`browse:${path ?? ""}`, (signal) => api.browse(path, signal));
  const current = listing.data?.path ?? path;
  // No listing has answered for the folder on screen yet, so Up reads the folder above it from the path itself.
  const up = listing.data ? listing.data.parent : current === null ? null : parentPath(current);

  return (
    <div className="browser">
      <div className="browser-bar">
        <Button small icon={<ArrowUp size={16} aria-hidden />} disabled={up === null} onClick={() => setPath(up)}>
          Up
        </Button>
        <code className="browser-path" title={current ?? undefined}>
          {current ?? "Loading…"}
        </code>
      </div>
      {/* One box in every state: the dialog is centred, so a box that changes size moves Up under the cursor. */}
      <div className="browser-body">
        {listing.status === "loading" ? (
          <Loading label="Loading folders" rows={4} />
        ) : listing.status === "error" ? (
          <ErrorState compact title="Couldn't read this folder" failure={listing.error} onRetry={listing.reload} />
        ) : listing.data.entries.length === 0 ? (
          <Empty title="No folders here" />
        ) : (
          <ul className="browser-list">
            {listing.data.entries.map((entry) => (
              <li key={entry.path}>
                <button type="button" onClick={() => setPath(entry.path)}>
                  {entry.is_git ? <GitBranch size={16} aria-hidden /> : <Folder size={16} aria-hidden />}
                  <span>{entry.name}</span>
                  {entry.is_git ? <span className="row-meta">Git repository</span> : null}
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>
      <div className="browser-foot">
        <Button
          tone="primary"
          busy={busy}
          disabled={!listing.data}
          onClick={() => {
            if (listing.data) onChoose(listing.data.path);
          }}
        >
          {actionLabel}
        </Button>
      </div>
    </div>
  );
}
