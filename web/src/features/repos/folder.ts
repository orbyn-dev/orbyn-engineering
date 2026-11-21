import { bridge } from "../../api/bridge";
import { inDesktopApp } from "../../api/shell";

/** The desktop shell command that opens the native folder dialog and answers the chosen path, or `null` when it was cancelled. */
export const PICK_FOLDER_COMMAND = "pick_folder";

/** Why a folder could not be chosen, in words that send the person to the path field. */
export class FolderPickFailure extends Error {
  constructor(message = "This version of the app cannot open the folder dialog. Type the path instead.") {
    super(message);
    this.name = "FolderPickFailure";
  }
}

/** True when this window can ask the desktop shell for a folder. A plain browser tab cannot, and keeps the path field alone. */
export const canPickFolder = (): boolean => inDesktopApp() && typeof bridge()?.invoke === "function";

/**
 * Asks the desktop shell for a folder in its native dialog. The answer is text from outside this page, so it is checked and handed back
 * as the path the person will read in the field: nothing here adds the folder, trusts it or runs anything in it.
 * Resolves `null` when the person cancelled.
 */
export async function pickFolder(): Promise<string | null> {
  const invoke = bridge()?.invoke;
  if (!inDesktopApp() || typeof invoke !== "function") throw new FolderPickFailure();
  let answer: unknown;
  try {
    answer = await invoke.call(bridge(), PICK_FOLDER_COMMAND);
  } catch {
    throw new FolderPickFailure();
  }
  if (answer === null || answer === undefined) return null;
  // eslint-disable-next-line no-control-regex
  if (typeof answer !== "string" || answer === "" || answer.length > 4096 || /[\u0000-\u001f\u007f]/.test(answer)) throw new FolderPickFailure("The folder dialog gave an answer this app cannot use. Type the path instead.");
  return answer;
}
