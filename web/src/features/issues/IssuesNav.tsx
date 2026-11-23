import { CircleDot } from "lucide-react";
import { routeHref } from "../../product/state/route";
import { useTrackers } from "./api";

/** The sidebar entry. It exists only while the service reports trackers; a service that reports none never shows it. */
export function IssuesNav({ current }: { current: boolean }) {
  const { rows } = useTrackers();
  if (rows === null) return null;
  return (
    <a className="nav-row" href={routeHref({ name: "issues" })} aria-current={current ? "page" : undefined}>
      <CircleDot size={15} aria-hidden />
      <span className="nav-row-label">Issues</span>
    </a>
  );
}
