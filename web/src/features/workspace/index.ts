// Front door: the only module other features import from workspace.
export { AddProjectPalette } from "./palette/AddProjectPalette";
export { GitHubStarPrompt, recordUserPrompt } from "./app-chrome/GitHubStarPrompt";
export { threadRunState } from "./sidebar/RunState";
export { archiveThreads, deleteThreadNow, settleThreads, unarchiveThread, unsettleThreads, wakeThreads } from "./sidebar/threadActions";
export { RenameThreadDialog, snoozeChildren } from "./sidebar/threadMenu";
export { CloneJobs } from "./app-chrome/CloneJobs";
export { EnvironmentIdentity } from "./app-chrome/EnvironmentIdentity";
export { FoldersNotice } from "./app-chrome/FoldersNotice";
export { QuitConfirmation } from "./app-chrome/QuitConfirmation";
export { CommandPalette } from "./palette/CommandPalette";
export { attentionRank, arrangeThreads, threadsFlat } from "./sidebar/groups";
export { Sidebar } from "./sidebar/Sidebar";
export { useDrawnOrder } from "./sidebar/drawnOrder";
export { copyReference, pinThreads, unpinThreads } from "./sidebar/threadActions";
export { releaseUndoShortcut, undoHasShortcut, undoLatest, useUndoNotice } from "./sidebar/threadUndo";
export { canStartThreadIn, readExplorerFile, startThreadIn, threadPathOf, useExplorer, useFileTabNames } from "./explorer";
