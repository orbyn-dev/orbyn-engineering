import { GitFork, MessageSquare, Bot } from "lucide-react";
import type { Thread } from "../../api/types";
import { routeHref } from "../../product/state/route";
import { useWorkspace } from "../../product/state/workspace";
import { parentLine } from "./model";
import "./family.css";

/** One quiet line in the header of a thread made from another: "Fork of <parent>". Nothing for any other thread. */
export function FamilyLine({ thread }: { thread: Thread }) {
  const threads = useWorkspace((state) => state.threads);
  const line = parentLine(thread, threads);
  if (!line) return null;
  const Icon = thread.lifecycle?.relation === "subagent" ? Bot : thread.lifecycle?.relation === "side" ? MessageSquare : GitFork;
  return (
    <span className="page-sub-item fam-line">
      <Icon size={13} aria-hidden />
      {line.relation} of{" "}
      <a href={routeHref({ name: "thread", id: line.parent.id })} title={line.parent.title}>
        {line.parent.title}
      </a>
    </span>
  );
}
