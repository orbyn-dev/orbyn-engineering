import { useCommand as useSharedCommand } from "../../../app/shortcuts";

/** Commands go to the thread's own composer; a side chat's stays out of their way. */
export const useOwnCommand = (side: boolean, command: string, handler: () => boolean | void) =>
  useSharedCommand(command, () => (side ? false : handler()));
