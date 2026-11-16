import { describeFrames } from "../shared/test-support";
import { BACKTEST_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "backtest-settings-unset": ["Crew › Reviewer backtest", "Nothing has run, and no figure is shown.", "What this tells you, and what it doesn’t", "Workers run as you, with your machine’s permissions."],
  "backtest-settings": ["Crew › Reviewer backtest", "All lanes and the checks", "What this tells you, and what it doesn’t", "Workers run as you, with your machine’s permissions."],
  "backtest-estimate": ["Main is healthy · watched since 09:00", "64 merged pull requests are 30 days old or more. 9 of them were reverted or fixed.", "Correctness · Claude Code · Sonnet 5.5", "Security · OpenCode · DeepSeek V4"],
  "backtest-over-budget": ["Main is healthy · watched since 09:00", "Correctness · Claude Code · Sonnet 5.5", "Use the last 180 days instead", "Choose the lane to replay. The others show Not run in the report."],
  "backtest-running": ["Main is healthy · watched since 09:00", "Correctness · Codex · GPT-5.5", "Sat 7 Nov, as of 16:31", "No result is shown until it has finished"],
  "backtest-stopped": ["Main is healthy · watched since 09:00", "You stopped this backtest at 40 of 64 pull requests.", "Correctness · Claude Code · Sonnet 5.5", "Security · OpenCode · DeepSeek V4"],
  "backtest-report": ["Main is healthy · watched since 09:00", "Counts alone: 9 escapes is a small sample", "Correctness · Claude Code · Sonnet 5.5", "What this tells you, and what it doesn’t"],
  "backtest-vendors": ["Main is healthy · watched since 09:00", "The same vendor as the builder", "Reviewer’s vendor against the builder’s", "reviewing a change built by Claude Code"],
  "backtest-escapes": ["Main is healthy · watched since 09:00", "Retry the calendar feed on a timeout", "Batch the digest emails", "8 of the 8 that were caught, so no escape depended on a single reviewer or check"],
  "backtest-unmatched": ["Main is healthy · watched since 09:00", "Three unmatched findings from Claude Code", "“The old key is still read in two places.”", "Unmatched isn’t the same as wrong"],
  "backtest-lane-failed": ["Main is healthy · watched since 09:00", "Escapes it would have caught", "of the escapes in these · too few for a rate", "What happens to the Codex row"],
  "backtest-empty": ["Main is healthy · watched since 09:00", "No pull requests with a known outcome yet", "Open Settings › Crew › Reviewer backtest"],
  "backtest-low-history": ["Main is healthy · watched since 09:00", "Escapes it would have caught", "Correctness · Codex · GPT-5.5", "Retry the feed sync on a timeout · fixed · caught by Claude Code and Codex"],
  "backtest-lane-picker": ["Correctness · Codex · Added by the brief", "Auto · a different engine from the author", "Backtest, Sat 7 Nov: caught 5 of 9 past escapes · 5 of 17 findings matched · 64 pull requests", "Not measured: it wasn’t in the last backtest"],
  "backtest-narrow-panel": ["64 pull requests replayed", "Would have caught 6 of 9 · recall 67%", "5 of 17 findings matched an escape", "It replays reviews and checks, never builders. The table, the escapes and the notes are behind Open in Crew."],
  "backtest-narrow-window": ["64 pull requests replayed", "Would have caught 6 of 9 · recall 67%", "Security · OpenCode · DeepSeek V4", "Failed on 7 pull requests, 3 of which escaped"],
};

describeFrames("Reviewer backtest frames (Crew Designs v23)", BACKTEST_FRAMES, SHOWS);
