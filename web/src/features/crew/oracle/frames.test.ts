import { describeFrames } from "../shared/test-support";
import { ORACLE_FRAMES } from "./names";

/** What each frame must show: text taken from its design. */
const SHOWS: Record<string, string[]> = {
  "oracle-settings": ["Crew › Independent oracle", "Brief and contract facts", "Tasks that change code · Proposed", "Workers run as you, with your machine’s permissions."],
  "oracle-languages": ["Crew › Independent oracle › Mutation tools", "None Crew supports yet", "Install cargo-mutants for this project", "Proposed: Engineering sets the real values"],
  "oracle-report": ["Add a rate limit to the login route", "Answer key check 4 · Claude Code · Sonnet 5.5 · exit 0 · 5d2e1a8", "Many addresses at once", "15 of 18 killed · 3 survived"],
  "oracle-key": ["Add a rate limit to the login route", "The lead wrote the brief, the scope and the contract facts for the login route.", "Check written by the author", "kills no mutant on its own"],
  "oracle-gate-running": ["Add a rate limit to the login route", "Another engine, another vendor · saw the brief and the contract facts, not the code", "11 of 18 mutants run so far", "Codex · GPT-5.5 · commit 5d2e1a8 · 3 files · +96 −4."],
  "oracle-gate-survivors": ["Add a rate limit to the login route", "The gate passed: the key killed 15 of 18 mutants.", "No check sends exactly five requests, so the edge of the limit is not tested.", "The check asserts that the header is there, not its value."],
  "oracle-gate-failed": ["Add a rate limit to the login route", "Sat 7 Nov, as of 13:30", "The 18 survivors, as the brief for stronger checks", "tests/auth/headers.test.ts"],
  "oracle-strengthen": ["Add a rate limit to the login route", "All 18 mutants survived.", "Written. The gate will run again on all 6 checks.", "Checks that fail on a survivor and pass on the real code"],
  "oracle-needs-you": ["Add a rate limit to the login route", "2 more checks added. The gate still killed no mutant.", "OpenCode · DeepSeek V4 starts from the brief and the contract facts, blind, like the first author.", "Ask OpenCode to write it"],
  "oracle-no-second-engine": ["Add a rate limit to the login route", "Codex · GPT-5.5. Crew looked for another engine to write the key and found none signed in.", "Sign in to another engine", "The checks the builder writes decide pass or fail, as before. The choice is logged."],
  "oracle-not-measured": ["Add a retry to the calendar feed", "tests/feed/retry_test.go", "Mutation gate: Not measured", "See supported languages"],
  "oracle-tool-missing": ["Handle the OAuth state mismatch", "Builder: Claude Code · Sonnet 5.5", "Rust · cargo-mutants not installed", "cargo install cargo-mutants"],
  "oracle-tool-error": ["Add a rate limit to the login route", "Claude Code · Sonnet 5.5", "written before the builder finished", "4 of 18 mutants run, then the tool stopped"],
  "oracle-dashboard": ["Main is healthy · watched since 09:00", "The builder is a different engine from the author in every row.", "mutants killed · too few gates for a rate", "Counts are exact, for this repo, on this device. A rate shows only from 10 gates. The numbers are sample data."],
  "oracle-empty": ["Main is healthy · watched since 09:00", "No independent answer keys in the last 60 days", "Open Settings › Crew › Independent oracle"],
  "oracle-low-sample": ["Main is healthy · watched since 09:00", "written by an engine other than the builder", "0 strengthened · 0 without the gate · 0 Not measured", "Update the calendar feed token · key by Claude Code · Sonnet 5.5 · TypeScript · 9 of 9 mutants killed"],
  "oracle-narrow-panel": ["Add a rate limit to the login route", "Brief and contract facts only, not the code", "auth/rate-limit.ts:41 · the edge of the limit", "The 6 checks and the timeline are behind Open in Crew."],
  "oracle-narrow-window": ["Add a rate limit to the login route", "Given only the brief and the contract facts", "The gate fails only when no mutant is killed.", "The check asserts that the header is there, not its value."],
};

describeFrames("Independent oracle frames (Crew Designs v22)", ORACLE_FRAMES, SHOWS);
