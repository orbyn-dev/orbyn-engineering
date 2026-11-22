import { MessageSquare, Wrench } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { ApiFailure } from "../../../api/client";
import { Button } from "../../../ui/controls";
import { earlierApi, type EarlierMessage, type EarlierPage } from "./earlierContract";

type Read =
  | { state: "idle" | "loading" }
  | { state: "ready"; page: EarlierPage; messages: EarlierMessage[]; more: boolean }
  | { state: "unreadable" };

const importedOn = (iso: string) => {
  const date = new Date(iso);
  return Number.isNaN(date.getTime()) ? "" : date.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
};

/**
 * What an imported thread said before it was imported, above a line saying where it came from. Read only: no buttons on the
 * messages, which cannot be edited or rewound. The read is skipped for a thread the record says was not imported, and the screen
 * draws nothing for one that was not imported, and on a failure that is not about the history.
 */
export function EarlierMessages({ threadId, imported }: { threadId: string; imported: boolean }) {
  const [read, setRead] = useState<Read>({ state: "idle" });
  const live = useRef<AbortController | null>(null);

  const load = useCallback(
    (before?: string) => {
      if (!imported) return;
      live.current?.abort();
      const control = new AbortController();
      live.current = control;
      if (before === undefined) setRead({ state: "loading" });
      earlierApi().read(threadId, before === undefined ? {} : { before }, control.signal).then(
        (page) => {
          if (control.signal.aborted) return;
          if (page === null) return setRead({ state: "idle" });
          setRead((now) => {
            const older = before !== undefined && now.state === "ready" ? now.messages : [];
            return { state: "ready", page, messages: [...page.messages, ...older], more: page.next_before !== null };
          });
        },
        (failure: unknown) => {
          if (control.signal.aborted) return;
          const changed = failure instanceof ApiFailure && (failure.code === "history_unreadable" || failure.code === "bad_cursor");
          setRead(changed ? { state: "unreadable" } : { state: "idle" });
        },
      );
    },
    [imported, threadId],
  );

  useEffect(() => {
    setRead({ state: "idle" });
    load();
    return () => live.current?.abort();
  }, [load]);

  if (read.state === "unreadable") {
    return (
      <div className="banner earlier-banner" role="status">
        <span>
          The earlier messages couldn’t be read. The session file has moved or changed. The agent still remembers them and carries on
          from where it stopped.
        </span>
        <Button small onClick={() => load()}>
          Try again
        </Button>
      </div>
    );
  }
  if (read.state !== "ready") return null;
  const { page, messages, more } = read;
  return (
    <section className="earlier" aria-label="Earlier messages">
      <div className="earlier-line" role="separator">
        <span>
          {page.source} · {page.total.toLocaleString()} earlier messages
        </span>
        {more ? (
          <Button small onClick={() => load(page.next_before ?? undefined)}>
            Show earlier
          </Button>
        ) : null}
      </div>
      {messages.map((message, at) => (
        <p key={at} data-testid="earlier-message" data-role={message.role} className={`earlier-message earlier-${message.role}`}>
          {message.role === "tool" ? <Wrench size={12} aria-hidden /> : message.role === "assistant" ? <MessageSquare size={12} aria-hidden /> : null}
          {message.text}
        </p>
      ))}
      <div className="earlier-line" role="separator">
        <span>
          Imported {importedOn(page.imported_at)} · continues in Orbyn
        </span>
      </div>
    </section>
  );
}
