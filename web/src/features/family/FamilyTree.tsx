import { Bot, GitFork, MessageSquare } from "lucide-react";
import type { KeyboardEvent } from "react";
import type { Thread } from "../../api/types";
import { useNow } from "../../lib/useNow";
import { navigate } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { relationLabel, rowStatus, type FamilyRow, type Relation } from "./model";
import "./family.css";

const ICONS: Record<Relation, typeof GitFork> = { fork: GitFork, side: MessageSquare, subagent: Bot };

/** The family as one tree, root first. A row opens its thread; titles and engine names are drawn as text. */
export function FamilyTree({ rows, onOpen }: { rows: readonly FamilyRow[]; onOpen?(): void }) {
  const engines = useWorkspace((state) => state.engines.data);
  const now = useNow(true, 60_000);
  const open = (thread: Thread) => {
    onOpen?.();
    navigate({ name: "thread", id: thread.id });
  };
  const key = (event: KeyboardEvent, thread: Thread) => {
    if (event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    open(thread);
  };
  return (
    <div className="fam-tree">
      {rows.map(({ thread, depth, relation, root, current }) => {
        const status = rowStatus(thread, now);
        const Icon = relation ? ICONS[relation] : null;
        return (
          <div
            key={thread.id}
            className="fam-row"
            role="link"
            tabIndex={0}
            aria-current={current ? "true" : undefined}
            style={{ paddingInlineStart: 10 + depth * 18 }}
            onClick={() => open(thread)}
            onKeyDown={(event) => key(event, thread)}
          >
            <span className="fam-title">
              {root ? <span className="fam-pill">Root</span> : null}
              {Icon ? (
                <span className="fam-pill">
                  <Icon size={12} aria-hidden />
                  {relationLabel(relation)}
                </span>
              ) : null}
              <b>{thread.title}</b>
              {current ? <span className="fam-quiet">This thread</span> : null}
            </span>
            <span className="fam-status" data-kind={status.kind}>
              {status.kind === "running" ? <i className="fam-dot" aria-hidden /> : null}
              {status.label}
            </span>
            <span className="fam-engine">{engines?.find((engine) => engine.id === thread.engine)?.name ?? thread.engine}</span>
          </div>
        );
      })}
    </div>
  );
}
