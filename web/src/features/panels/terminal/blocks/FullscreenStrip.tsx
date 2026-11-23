import { cleanCommand } from "./blockText";

/**
 * What the blocks list shows while a full-screen program runs: one line naming the command, and when the blocks come back. The
 * program's own block returns, as the finished full-screen card, once it exits.
 */
export function FullscreenStrip({ command }: { command: string | null }) {
  const line = cleanCommand(command);
  return (
    <div className="cb-strip" role="status">
      <b>{line === null ? "Full-screen program running" : `Full-screen program running · ${line}`}</b>{" "}
      <span>Blocks resume when it exits</span>
    </div>
  );
}
