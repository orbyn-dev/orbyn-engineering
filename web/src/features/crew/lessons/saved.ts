/**
 * The one lesson a person has just added by hand, so the card it opens on can say "Saved." once. It is memory only,
 * wiped on a reload, and consumed by the card it names: it is a hand-off between two screens, never stored data.
 */
let added: string | null = null;

export const markAdded = (id: string): void => {
  added = id;
};

/** Whether this card is the one the Add page just made, taking the mark so it is drawn once. */
export function takeAdded(id: string): boolean {
  if (added !== id) return false;
  added = null;
  return true;
}
