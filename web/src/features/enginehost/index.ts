// Front door of the engine host feature: the only module other features import from it. The host's state, what an engine's gap
// is called, and the pieces the thread draws (the picker's state rows and the composer's line). The Settings screens that
// show the same facts live in `features/settings/engineHost` and read them from here.
export { engineHostApi, hostWritable, setEngineHostApi, useEngineHostCrashes, useEngineHosts, useKeyStorage } from "./hostApi";
export { cannotSend, engineGap, pickerGap, programOf, sendsWithoutHost } from "./gaps";
export type { Gap, GapAction, GapKind } from "./gaps";
export {
  bannerLine, cantStartLead, capabilityLine, CHIPS, chipFor, clock, errorKindLine, exitLine, notUsedReason, isProcess, listLine, mismatchLine, plain, processLine, restartLine, runningFor, shortDate, stateLine, STOPPED_SENTENCE,
} from "./hostText";
export type { Chip } from "./hostText";
export { BACKGROUND_OPEN, KEYCHAIN_DENIED, KEYCHAIN_LOCKED } from "./contract";
export { isBackgroundOpen } from "./hostControl";
export type {
  EngineHostApi, HostCause, HostControlOptions, HostCrash, HostError, HostErrorKind, HostRow, HostState, KeyStorage, StorageChoice,
} from "./contract";
export { ClaudePathLine, ComposerHostLine, PickerGapRow, useAppUpdateOrbyn, useHostFor, useHostLine } from "./HostUi";
export type { HostLine } from "./HostUi";
