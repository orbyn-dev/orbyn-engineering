/**
 * What a failed clone's toast says. The service sends Git's own output (`fatal: ...`), which is
 * meant for a terminal; a person gets one short plain sentence they can act on instead.
 */

/** Git could not read the source at all: a malformed URL, or a repository that is not there. */
const INVALID_ADDRESS =
  /does not exist|does not appear to be a git repository|not a git repository|bad\/illegal format|bad url|no such file or directory|repository name is invalid/i;

/** Git reached the server and the repository is absent, or this computer is not allowed to read it: a 401, 403 or 404 answer lands here. */
const NO_ACCESS =
  /not found|authentication|permission denied|publickey|could not read (username|from remote)|access denied|invalid username or password|\b40[134]\b/i;

/** Git could not reach the server: a name that does not resolve, a refused or timed-out connection. */
const UNREACHABLE =
  /could not resolve host|failed to connect|couldn't connect|connection (refused|timed out|was reset)|network is unreachable|no route to host|temporary failure in name resolution|operation timed out|timed out|ssl certificate|unable to access|recv failure/i;

const INVALID_WORDS = "That address is not a Git repository.";
const NO_ACCESS_WORDS = "The repository was not found, or this computer has no access to it.";
const UNREACHABLE_WORDS = "Orbyn could not reach the repository. Check the connection, then try again.";
/** A failure none of the shapes above names: still plain, still says what to try. */
const UNKNOWN_WORDS = "Check the address and the connection, then try again.";

export function cloneFailure(error: string): string {
  if (INVALID_ADDRESS.test(error)) return INVALID_WORDS;
  if (NO_ACCESS.test(error)) return NO_ACCESS_WORDS;
  if (UNREACHABLE.test(error)) return UNREACHABLE_WORDS;
  return UNKNOWN_WORDS;
}
