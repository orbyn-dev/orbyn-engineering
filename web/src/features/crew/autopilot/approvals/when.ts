// When a Crew read's RFC 3339 time is drawn, in the person's own clock: the list and the Background screens share these.
const WEEKDAY = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const MONTH = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const pad = (n: number) => String(n).padStart(2, "0");
const dayOf = (at: Date) => `${WEEKDAY[at.getDay()]} ${at.getDate()} ${MONTH[at.getMonth()]}`;
const dayMonthOf = (at: Date) => `${at.getDate()} ${MONTH[at.getMonth()]}`;
const sameDay = (a: Date, b: Date) => a.toDateString() === b.toDateString();

/** "15:11" in the person's clock. */
export const clockOf = (iso: string): string => {
  const at = new Date(iso);
  return `${pad(at.getHours())}:${pad(at.getMinutes())}`;
};
/** "Fri 6 Nov" in the person's clock. */
export const dayOfIso = (iso: string): string => dayOf(new Date(iso));
/** "15:11" for today, "Fri 6 Nov" before. */
export function whenShort(iso: string, now: Date): string {
  const at = new Date(iso);
  return sameDay(at, now) ? `${pad(at.getHours())}:${pad(at.getMinutes())}` : dayOf(at);
}
/** "Sat 7 Nov, 15:11". */
export function whenLong(iso: string): string {
  const at = new Date(iso);
  return `${dayOf(at)}, ${pad(at.getHours())}:${pad(at.getMinutes())}`;
}
/** "14:02" for the same local day, "5 Nov, 16:02" otherwise: the way a change names the row it resolved. */
export function whenMoment(iso: string, now: Date): string {
  const at = new Date(iso);
  return sameDay(at, now) ? `${pad(at.getHours())}:${pad(at.getMinutes())}` : `${dayMonthOf(at)}, ${pad(at.getHours())}:${pad(at.getMinutes())}`;
}
/** The two RFC 3339 times name the same instant, to the second. */
export const sameMoment = (one: string | null | undefined, other: string | null | undefined): boolean =>
  one != null && other != null && Math.abs(Date.parse(one) - Date.parse(other)) < 1000;
/** `iso` shifted by `hours`: when the next check of a watch with that schedule is due. */
export const plusHours = (iso: string, hours: number): string => new Date(Date.parse(iso) + hours * 3_600_000).toISOString();
