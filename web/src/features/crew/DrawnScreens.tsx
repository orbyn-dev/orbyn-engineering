import type { ComponentType } from "react";
import { WATCH_SCREENS } from "./autopilot/approvals/watchScreens";
import { BACKTEST_LIVE_SCREENS } from "./backtest/live/screens";
import { BACKTEST_SCREENS } from "./backtest/screens";
import { CAPACITY_LIVE_SCREENS } from "./capacity/live/screens";
import { CAPACITY_SCREENS } from "./capacity/screens";
import { DASHBOARD_SCREENS } from "./dashboard/screens";
import { ESCAPE_LIVE_SCREENS } from "./escape/live/screens";
import { ESCAPE_SCREENS } from "./escape/screens";
import { FLOW_SCREENS } from "./flow/screens";
import { NEEDSYOU_SCREENS } from "./needsyou/screens";
import { ORACLE_SCREENS } from "./oracle/screens";
import { SETTINGS_SCREENS } from "./settings/screens";
import { PreviewNav } from "./shared/preview-nav";
import "./shared/frame.css";

const DRAWN: Record<string, ComponentType> = { ...ESCAPE_SCREENS, ...ESCAPE_LIVE_SCREENS, ...CAPACITY_SCREENS, ...CAPACITY_LIVE_SCREENS, ...ORACLE_SCREENS, ...BACKTEST_SCREENS, ...BACKTEST_LIVE_SCREENS, ...SETTINGS_SCREENS, ...FLOW_SCREENS, ...DASHBOARD_SCREENS, ...WATCH_SCREENS };

/** The frames of Crew Designs v20 to v23, the escape frames of v79 and the backtest frames of v23 on their real routes, and the Flow frames (Version 35). A separate chunk, so opening a review frame never loads them. */
export default function DrawnScreens({ name }: { name: string }) {
  const Screen = DRAWN[name] ?? NEEDSYOU_SCREENS[name as keyof typeof NEEDSYOU_SCREENS];
  return Screen ? (
    <main className="cx-root">
      <PreviewNav>
        <Screen />
      </PreviewNav>
    </main>
  ) : null;
}
