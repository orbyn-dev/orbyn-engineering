import type { ComponentType } from "react";
import { OracleSettings } from "./settings";
import { OracleLanguages } from "./languages";
import { OracleReport } from "./report";
import { OracleKey } from "./key";
import { OracleGateRunning } from "./gate-running";
import { OracleGateSurvivors } from "./gate-survivors";
import { OracleGateFailed } from "./gate-failed";
import { OracleStrengthen } from "./strengthen";
import { OracleNeedsYou } from "./needs-you";
import { OracleNoSecondEngine } from "./no-second-engine";
import { OracleNotMeasured } from "./not-measured";
import { OracleToolMissing } from "./tool-missing";
import { OracleToolError } from "./tool-error";
import { OracleDashboard } from "./dashboard";
import { OracleEmpty } from "./empty";
import { OracleLowSample } from "./low-sample";
import { OracleNarrowPanel } from "./narrow-panel";
import { OracleNarrowWindow } from "./narrow-window";
import type { OracleFrameId } from "./names";

/** Each frame of Crew Designs v22, drawn on sample data. Loaded only through the Preview route. */
export const ORACLE_SCREENS: Record<OracleFrameId, ComponentType> = {
  "oracle-settings": OracleSettings,
  "oracle-languages": OracleLanguages,
  "oracle-report": OracleReport,
  "oracle-key": OracleKey,
  "oracle-gate-running": OracleGateRunning,
  "oracle-gate-survivors": OracleGateSurvivors,
  "oracle-gate-failed": OracleGateFailed,
  "oracle-strengthen": OracleStrengthen,
  "oracle-needs-you": OracleNeedsYou,
  "oracle-no-second-engine": OracleNoSecondEngine,
  "oracle-not-measured": OracleNotMeasured,
  "oracle-tool-missing": OracleToolMissing,
  "oracle-tool-error": OracleToolError,
  "oracle-dashboard": OracleDashboard,
  "oracle-empty": OracleEmpty,
  "oracle-low-sample": OracleLowSample,
  "oracle-narrow-panel": OracleNarrowPanel,
  "oracle-narrow-window": OracleNarrowWindow,
};
