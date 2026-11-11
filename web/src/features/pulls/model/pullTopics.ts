/** `pull:<project>:<number>`, read apart; a project id may hold colons, so the number is what follows the last one. */
export function parsePullTopic(what: string): { project: string; number: number } | null {
  if (!what.startsWith("pull:")) return null;
  const rest = what.slice("pull:".length);
  const at = rest.lastIndexOf(":");
  if (at <= 0) return null;
  const number = Number(rest.slice(at + 1));
  return Number.isInteger(number) ? { project: rest.slice(0, at), number } : null;
}

/**
 * What says one pull request changed: its own topic, and `pulls`, which the service sends when a
 * push, a workflow approval or a run's end dropped every read of the project.
 */
export const pullChanged = (project: string, number: number) => (what: string) => {
  if (what === "pulls") return true;
  const topic = parsePullTopic(what);
  return topic !== null && topic.project === project && topic.number === number;
};

/** What says any pull request of the project changed: the stack of one reads the whole project's. */
export const projectPullsChanged = (project: string) => (what: string) => what === "pulls" || parsePullTopic(what)?.project === project;
