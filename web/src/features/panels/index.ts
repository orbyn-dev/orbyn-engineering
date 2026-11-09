// Front door: the only module other features import from panels.
export { GitActionDialog, PLANS, type Plan, PublishDialog, createPullRequestPlan, useGitAvailable } from "./GitActionDialog";
export { RestoreFilesDialog } from "./RestoreFilesDialog";
export { SurfacePane } from "./SurfacePane";
export { TerminalBar, TerminalDrawerBody } from "./TerminalDrawerBody";
