import type { ComponentType } from "react";
import { BACKTEST_SCREENS } from "./backtest/screens";
import { CAPACITY_SCREENS } from "./capacity/screens";
import { ESCAPE_SCREENS } from "./escape/screens";
import { ORACLE_SCREENS } from "./oracle/screens";
import { SETTINGS_SCREENS } from "./settings/screens";
import "./shared/frame.css";

const DRAWN: Record<string, ComponentType> = { ...ESCAPE_SCREENS, ...CAPACITY_SCREENS, ...ORACLE_SCREENS, ...BACKTEST_SCREENS, ...SETTINGS_SCREENS };

/** The frames of Crew Designs v20 to v23. A separate chunk, so opening a review frame never loads them. */
export default function DrawnScreens({ name }: { name: string }) {
  const Screen = DRAWN[name];
  return Screen ? (
    <main className="cx-root">
      <Screen />
    </main>
  ) : null;
}
