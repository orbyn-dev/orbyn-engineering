import type { ReactElement, ReactNode } from "react";
import type { EngineStatus } from "../../../api/types";
import type { ProviderEntry } from "../../../generated/ProviderEntry";
import type { ClaudeEnginePath } from "../../../product/engines/claudePath";
import { useHostFor } from "../../enginehost";
import { HostChip } from "../engineHost/HostParts";

import { CLAUDE_CLI_NOTE, CLAUDE_SDK_NOTE } from "../../../product/engines/claudePath";

/** The helper line under each Claude row: what that path is, in the provider list and the engine picker. */
export { CLAUDE_CLI_NOTE, CLAUDE_SDK_NOTE };

export interface ClaudePathRow {
  key: string;
  name: string;
  note: string;
  /** The path this row stands for: only its row carries the In use switch. */
  path: ClaudeEnginePath;
  /** The SDK row's host state; the CLI row has none. */
  chip?: ReactNode;
}

/** The one Claude engine has two list rows, one per path; every other engine keeps its one row. */
export function claudePathRows(
  engine: EngineStatus,
  entry: ProviderEntry | undefined,
  render: (engine: EngineStatus, entry: ProviderEntry | undefined, row?: ClaudePathRow) => ReactElement,
): ReactElement[] | null {
  if (engine.instance_of || engine.id !== "claude") return null;
  return [
    render(engine, entry, {
      key: "claude-sdk",
      name: "Claude (SDK)",
      note: CLAUDE_SDK_NOTE,
      path: "sdk",
      chip: <ClaudeHostChip engineId={engine.id} />,
    }),
    render(engine, entry, { key: "claude-cli", name: "Claude (CLI)", note: CLAUDE_CLI_NOTE, path: "cli" }),
  ];
}

/** The SDK row's extra chip: the same host state the Engine host row shows on the Claude Code page. */
function ClaudeHostChip({ engineId }: { engineId: string }) {
  const host = useHostFor(engineId);
  return host ? <HostChip state={host.state} /> : null;
}
