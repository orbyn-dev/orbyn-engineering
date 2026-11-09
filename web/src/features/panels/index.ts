// Front door: the only module other features import from panels.
export { GitActionDialog, PublishDialog, useGitAvailable } from "./diff/GitActionDialog";
export { RestoreFilesDialog } from "./diff/RestoreFilesDialog";
export { PanelHost } from "./PanelHost";
export { TerminalBar, TerminalDrawerBody } from "./terminal/TerminalDrawerBody";
