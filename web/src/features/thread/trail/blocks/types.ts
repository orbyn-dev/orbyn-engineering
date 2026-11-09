import type { ReactNode } from "react";
import type { ToolItem } from "../../../../product/timeline/types";
import type { useUi } from "../../../../product/state/ui";
import type { callPath } from "../../toolCalls";
import type { parsed } from "../../../../product/timeline/toolTrace";

/** What Block works out once and hands to the renderer for the call's kind. */
export interface BlockContext {
  item: ToolItem;
  running: boolean;
  cwd: string | null;
  origin: { threadId: string; cwd: string };
  body: ReturnType<typeof parsed>;
  duration: string | null;
  exit: ToolItem["exitCode"];
  failed: boolean;
  out: string | undefined;
  path: ReturnType<typeof callPath>;
  shown: string | null;
  stopped: ReactNode;
  revealFile: ReturnType<typeof useUi.getState>["revealFile"];
  revealDiff: ReturnType<typeof useUi.getState>["revealDiff"];
  showInBrowser: ReturnType<typeof useUi.getState>["showInBrowser"];
  /** The footer line: the parts given, then the duration. */
  timing(...parts: ReactNode[]): ReactNode;
}
