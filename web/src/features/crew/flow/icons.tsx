import type { CSSProperties, ReactNode } from "react";

/** The icons the Flow frames draw, as the design's own symbols (24 by 24, stroked with the text colour). */
const PATHS = {
  activity: (<><path d="M3.5 12h4l2.5-6 4 12 2.5-6h4" /></>),
  alert: (<><path d="M12 4l9 15.5H3z" /><path d="M12 10v4M12 17h.01" /></>),
  branch: (<><circle cx="6.5" cy="5.5" r="2" /><circle cx="6.5" cy="18.5" r="2" /><circle cx="17.5" cy="7.5" r="2" /><path d="M6.5 7.5v9M17.5 9.5c0 4.5-5 4.5-9.5 7.5" /></>),
  brief: (<><rect x="4" y="4.5" width="16" height="15" rx="2" /><path d="M8 9h8M8 12.5h8M8 16h5" /></>),
  bug: (<><rect x="7.5" y="8" width="9" height="11" rx="4.5" /><path d="M12 8v11M4 12h3.5M16.5 12H20M5 7l2.5 2M19 7l-2.5 2M5 18l2.5-2M19 18l-2.5-2M9.5 8a2.5 2.5 0 0 1 5 0" /></>),
  check2: (<><path d="M5.5 12.5l4 4 9-9" /></>),
  clock: (<><circle cx="12" cy="12" r="8.5" /><path d="M12 7.5V12l3 2" /></>),
  cpu: (<><rect x="6.5" y="6.5" width="11" height="11" rx="1.5" /><path d="M9.5 3.5v3M14.5 3.5v3M9.5 17.5v3M14.5 17.5v3M3.5 9.5h3M3.5 14.5h3M17.5 9.5h3M17.5 14.5h3" /></>),
  diff: (<><path d="M14 3.5H7.5a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2h9a2 2 0 0 0 2-2V8z" /><path d="M14 3.5V8h4.5M9.5 11.5h5M12 9v5M9.5 16.5h5" /></>),
  down: (<><path d="M7 10l5 5 5-5" /></>),
  eye: (<><path d="M2.5 12S6 5.5 12 5.5 21.5 12 21.5 12 18 18.5 12 18.5 2.5 12 2.5 12z" /><circle cx="12" cy="12" r="2.8" /></>),
  flow: (<><rect x="3.5" y="4" width="6" height="5" rx="1.2" /><rect x="14.5" y="15" width="6" height="5" rx="1.2" /><path d="M6.5 9v4.5a2 2 0 0 0 2 2h6" /></>),
  gate: (<><path d="M5 20V8a7 7 0 0 1 14 0v12M5 13h14M12 13v7" /></>),
  git: (<><circle cx="12" cy="12" r="3" /><path d="M3.5 12H9M15 12h5.5" /></>),
  lock: (<><rect x="5" y="10.5" width="14" height="9.5" rx="2" /><path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" /></>),
  max: (<><path d="M14 4h6v6M10 20H4v-6M20 4l-7 7M4 20l7-7" /></>),
  more: (<><path d="M6 12h.01M12 12h.01M18 12h.01" /></>),
  ok: (<><circle cx="12" cy="12" r="8.5" /><path d="M8.5 12.5l2.5 2.5 4.5-5" /></>),
  play: (<><path d="M8 5.5v13l10.5-6.5z" /></>),
  plus: (<><path d="M12 5v14M5 12h14" /></>),
  pr: (<><circle cx="6" cy="6" r="2.2" /><circle cx="6" cy="18" r="2.2" /><circle cx="18" cy="18" r="2.2" /><path d="M6 8.2v7.6M18 15.8V9.5A3 3 0 0 0 15 6.5h-4M13 4.5l-2 2 2 2" /></>),
  right: (<><path d="M9.5 6l6 6-6 6" /></>),
  scan: (<><path d="M4 8.5V6a2 2 0 0 1 2-2h2.5M15.5 4H18a2 2 0 0 1 2 2v2.5M20 15.5V18a2 2 0 0 1-2 2h-2.5M8.5 20H6a2 2 0 0 1-2-2v-2.5M7.5 12h9" /></>),
  search: (<><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></>),
  shield: (<><path d="M12 3.5l7 2.8v5.4c0 4.3-3 7.5-7 8.8-4-1.3-7-4.5-7-8.8V6.3z" /></>),
  side: (<><path d="M4 6.5h9a2 2 0 0 1 2 2V14a2 2 0 0 1-2 2H9l-3 2.5V16H4" /><path d="M15 9.5h3.5a2 2 0 0 1 2 2V17h-1.5v2l-2.5-2H12" /></>),
  user: (<><circle cx="12" cy="8.5" r="3.5" /><path d="M5 20c1-3.5 3.8-5.5 7-5.5s6 2 7 5.5" /></>),
  wrench: (<><path d="M14.5 5.5a4 4 0 0 0 4.9 4.9L11 18.8a1.8 1.8 0 0 1-2.6-2.6l8.4-8.4" /></>),
  x: (<><path d="M7 7l10 10M17 7L7 17" /></>),
} satisfies Record<string, ReactNode>;

export type FlowIconName = keyof typeof PATHS;

export function Ic({ n, sm, className, style }: { n: FlowIconName; sm?: boolean; className?: string; style?: CSSProperties }) {
  return (
    <svg className={["cx-ic", sm ? "cx-sm" : "", className].filter(Boolean).join(" ")} viewBox="0 0 24 24" style={style} aria-hidden="true">
      {PATHS[n]}
    </svg>
  );
}
