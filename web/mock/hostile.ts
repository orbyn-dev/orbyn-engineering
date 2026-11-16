/**
 * What a hostile service adds where the contract has no field, one marker for each, so a leak path can be told from another. No
 * screen may draw any of them: not as text, and not in an attribute (a title, an aria label, a data attribute, a spread prop).
 */
export const HOSTILE = {
  body: "HOSTILE-MESSAGE-BODY-7731",
  text: "HOSTILE-MESSAGE-TEXT-5518",
  value: "HOSTILE-FACT-VALUE-9042",
  thread: "HOSTILE-THREAD-ID-3306",
  worktree: "HOSTILE-WORKTREE-PATH-2284",
  tokens: "HOSTILE-TOKENS-4471",
  level: "HOSTILE-LEVEL-6139",
} as const;
export const HOSTILE_MARKERS: readonly string[] = Object.values(HOSTILE);
export const HOSTILE_BODY = HOSTILE.body;
export const HOSTILE_VALUE = HOSTILE.value;
