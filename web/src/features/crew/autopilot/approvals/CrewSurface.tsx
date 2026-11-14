import { useId, useState } from "react";
import { CREW_TABS as TABS, routeHref, type CrewTab } from "../../../../product/state/route";
import { PageBar } from "../../../../product/ui/PageBar";
import { BoardView } from "../../board/BoardView";
import { CrewTabs } from "../../flow/live/CrewTabs";
import { FlowView } from "../../flow/live/FlowView";
import { NeedsYouInbox } from "../../needsyou/NeedsYouInbox";
import { ApprovalsTab } from "./ApprovalsTab";
import { BackgroundTab } from "./Background";
import { FirstRunBanner } from "./FirstRunBanner";
import { Chip } from "./parts";
import { TrustTab } from "./Trust";
import { useCrewPreview } from "./useCrew";

const LABEL: Record<CrewTab, string> = { approvals: "Approvals", trust: "Trust", background: "Background", inbox: "Inbox", flow: "Flow", board: "Board" };
export const CREW_TABS: { id: CrewTab; label: string }[] = TABS.map((id) => ({ id, label: LABEL[id] }));

/**
 * The Crew screens this slice owns: the approval log, trust and background crews. Nothing is drawn while
 * Crew Preview is off. The other Crew tabs (Flow, Review and the rest) are built elsewhere and join this list.
 */
export function CrewSurface({ tab: opened }: { tab: CrewTab }) {
  const on = useCrewPreview();
  // The tab is the screen's own state: the address follows it without a route change, so the tab list keeps focus.
  const [tab, setTab] = useState(opened);
  const select = (next: CrewTab) => {
    setTab(next);
    window.history.replaceState(window.history.state, "", routeHref({ name: "crew", tab: next }));
  };
  const id = useId();
  if (!on) return null;
  return (
    <div className="ap-surface">
      <PageBar>
        <h1>Crew</h1>
        <Chip tone="plain">Preview</Chip>
      </PageBar>
      <FirstRunBanner />
      <CrewTabs tabs={CREW_TABS} active={tab} onSelect={select} base={id} />
      <div className="ap-panel" role="tabpanel" id={`${id}-panel`} aria-labelledby={`${id}-${tab}`}>
        {tab === "approvals" ? <ApprovalsTab /> : tab === "trust" ? <TrustTab /> : tab === "inbox" ? <NeedsYouInbox /> : tab === "flow" ? <FlowView /> : tab === "board" ? <BoardView /> : <BackgroundTab />}
      </div>
    </div>
  );
}
