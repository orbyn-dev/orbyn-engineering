/**
 * The one thing that has to travel between two screens: the skill a person just approved. The review page leaves for the
 * Lessons page, which says "Added to skills-local" once, naming the skill; the Lessons page shows the same line when the
 * approval happened in its own confirm. Memory only, wiped on a reload, consumed by the page that draws it.
 */
let added: { name: string; lessons: number } | null = null;

export const markSkillAdded = (name: string, lessons: number): void => {
  added = { name, lessons };
};

/** The skill that was just added, taking the mark so the line is drawn once. */
export function takeSkillAdded(): { name: string; lessons: number } | null {
  const mark = added;
  added = null;
  return mark;
}

/** Forgets what was marked, for a test that starts from nothing. */
export function forgetSkillAdded(): void {
  added = null;
}
