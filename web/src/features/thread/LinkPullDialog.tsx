import { useState } from "react";
import type { Thread } from "../../api/types";
import { toFailure, useAsync } from "../../lib/useAsync";
import { Dialog } from "../../ui/Dialog";
import { pullsApi } from "../../api/pulls";

/** Finds a pull request by number or title, and links it to this thread. */
export function LinkPullDialog({ thread, onClose, onLinked }: { thread: Thread; onClose(): void; onLinked(): void }) {
  const [query, setQuery] = useState("");
  const candidates = useAsync(`pull-candidates:${thread.project_id}:${query}`, (signal) =>
    pullsApi.projectPulls(thread.project_id, query, signal),
  );
  const [error, setError] = useState<string | null>(null);
  const link = (number: number) => {
    setError(null);
    pullsApi.link(thread.id, number).then(
      () => {
        onLinked();
        onClose();
      },
      (failure: unknown) => setError(toFailure(failure).message),
    );
  };
  return (
    <Dialog title="Link a pull request" onClose={onClose}>
      <input aria-label="Find a pull request" placeholder="Number or title" value={query} autoFocus onChange={(event) => setQuery(event.target.value)} />
      <ul className="branch-list" aria-label="Pull requests">
        {(candidates.data ?? []).map((pull) => (
          <li key={pull.number}>
            <button type="button" onClick={() => link(pull.number)}>
              <span>
                #{pull.number} {pull.title}
              </span>
            </button>
          </li>
        ))}
      </ul>
      {error ? (
        <p className="field-error" role="alert">
          {error}
        </p>
      ) : null}
    </Dialog>
  );
}
