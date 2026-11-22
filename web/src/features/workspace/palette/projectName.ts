/** Windows keeps these names for its devices, so no folder can be called one. */
const RESERVED_NAMES = new Set([
  "CON", "PRN", "AUX", "NUL",
  "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9",
  "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
]);

/** A letter, a number, a space, `-`, `_` or `.`; the service refuses the rest too. */
function allowed(character: string): boolean {
  return /[\p{L}\p{N}]/u.test(character) || character === " " || character === "-" || character === "_" || character === ".";
}

/**
 * Why this name cannot become a project folder, or `null` when it can. The New project
 * dialog's Name is the folder's own name, so it is checked here before the dialog
 * submits, in the same words the service refuses it with.
 */
export function projectNameProblem(name: string): string | null {
  const trimmed = name.trim();
  // An empty field is not a mistake yet: Create is already off and the field is untouched.
  if (!trimmed) return null;
  if ([...trimmed].some((character) => !allowed(character))) {
    return "A project name cannot contain / \\ : * ? \" < > |.";
  }
  if (trimmed.startsWith(".")) return "A project name cannot start with a dot.";
  if (trimmed.endsWith(".")) return "A project name cannot end with a dot.";
  if (RESERVED_NAMES.has(trimmed.toUpperCase())) return "Windows reserves that project name.";
  return null;
}
