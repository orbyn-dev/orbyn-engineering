import type { PreferenceEdit } from "../../../generated/PreferenceEdit";
import type { ScopedPreferenceRow } from "../../../generated/ScopedPreferenceRow";

export type PreferenceSaveResult = { saved: true; row: ScopedPreferenceRow | undefined } | { saved: false; error: string | null };
export interface PreferenceControl {
  disabled: boolean;
  mixed: boolean;
  commit(edit: PreferenceEdit): Promise<PreferenceSaveResult>;
}

export interface PreferenceRow {
  key: string;
  title: string;
  description?: string;
  /** A description that follows another row's current value, given any row's value by key. Wins over `description`. */
  describe?(valueOf: (key: string) => unknown): string | undefined;
  /** What an empty text field says. */
  placeholder?: string;
  /** Only where the preference response describes the whole inheritance chain. */
  inheritance?: boolean;
  /** Value and label, for a fixed choice. */
  options?: { value: string; label: string }[];
  /** Shows a short fixed choice side by side instead of as a menu. */
  segmented?: boolean;
  parse?(value: string): unknown;
  multiline?: boolean;
  /** Hides the row unless this holds, given any row's current value by key. */
  showWhen?(valueOf: (key: string) => unknown): boolean;
  /** For a value none of the plain controls fit. */
  render?(value: unknown, save: (next: unknown) => void, control: PreferenceControl): React.ReactNode;
}
