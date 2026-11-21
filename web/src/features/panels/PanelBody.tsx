import type { ReactNode } from "react";
import type { Thread } from "../../api/types";
import { isFileTab, type RightTab } from "../../product/state/ui";
import { DevicesPanel } from "../devices";
import { LibraryPanel } from "../library";
import { insertIntoTerminal } from "./terminal/terminalControls";
import { ToolsTab } from "../tools";
import { ActivityPanel } from "./activity/ActivityPanel";
import { BrowserPanel } from "./browser/BrowserPanel";
import { DiffPanel } from "./diff/DiffPanel";
import { FileTab } from "./files/FileTab";
import { ReviewPanel } from "./review/ReviewPanel";

/** What the open tab shows. The side chat and linked threads are the thread's own and arrive as `content`. */
export function PanelBody({ tab, thread, content }: { tab: RightTab; thread: Thread; content: { side?: ReactNode; linked?: ReactNode } }) {
  if (tab === "browser") return <BrowserPanel threadId={thread.id} />;
  if (tab === "diff") return <DiffPanel threadId={thread.id} />;
  if (isFileTab(tab)) return <FileTab key={tab} id={tab} threadId={thread.id} />;
  if (tab === "activity") return <ActivityPanel key={thread.id} threadId={thread.id} />;
  if (tab === "tools") return <ToolsTab key={thread.id} threadId={thread.id} engine={thread.engine} />;
  if (tab === "devices") return <DevicesPanel key={thread.id} />;
  if (tab === "library") return <LibraryPanel key={thread.id} threadId={thread.id} typeIntoTerminal={insertIntoTerminal} />;
  if (tab === "review") return <ReviewPanel key={thread.id} threadId={thread.id} />;
  return content[tab as "side" | "linked"];
}
