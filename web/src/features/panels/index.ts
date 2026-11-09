// Front door: the only module other features import from panels.
export { GitActionDialog, PLANS, type Plan, PublishDialog, createPullRequestPlan, useGitAvailable } from "./diff/GitActionDialog";
export { RestoreFilesDialog } from "./diff/RestoreFilesDialog";
export { SurfacePane } from "./SurfacePane";
export { TerminalBar, TerminalDrawerBody } from "./terminal/TerminalDrawerBody";
