import type { AuthState, RunState } from "../../api/types";

export const runStateLabels: Record<RunState, string> = {
  admitted: "Queued",
  starting: "Starting",
  running: "Running",
  awaiting_approval: "Waiting for you",
  completed: "Completed",
  failed: "Failed",
  cancelled: "Cancelled",
  interrupted: "Interrupted",
};

export const authLabels: Record<AuthState, string> = {
  logged_in: "Signed in",
  logged_out: "Signed out",
  unknown: "Login unknown",
  not_applicable: "No login needed",
};
