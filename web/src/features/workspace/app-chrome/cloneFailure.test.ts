import { expect, it } from "vitest";
import { cloneFailure } from "./cloneFailure";

const shown = (raw: string) => {
  const said = cloneFailure(raw);
  // Never Git's own output and never more than the one sentence the toast holds.
  expect(said).not.toContain("fatal:");
  expect(said).not.toContain("\n");
  expect(said.endsWith(".")).toBe(true);
  return said;
};

it.each([
  ["fatal: repository 'bad url with spaces' does not exist", "That address is not a Git repository."],
  ["fatal: unable to access 'https://example.com/team/repo.git/': URL using bad/illegal format or missing URL", "That address is not a Git repository."],
  ["fatal: repository 'https://example.com/team/repo.git/' not found", "The repository was not found, or this computer has no access to it."],
  ["remote: Repository not found.\nfatal: repository 'https://github.com/orbyn-dev/private.git/' not found", "The repository was not found, or this computer has no access to it."],
  ["fatal: Authentication failed for 'https://github.com/orbyn-dev/private.git/'", "The repository was not found, or this computer has no access to it."],
  ["fatal: could not read Username for 'https://github.com': terminal prompts disabled", "The repository was not found, or this computer has no access to it."],
  ["git@github.com: Permission denied (publickey).\nfatal: Could not read from remote repository.", "The repository was not found, or this computer has no access to it."],
  ["fatal: unable to access 'https://github.com/orbyn-dev/private.git/': The requested URL returned error: 403", "The repository was not found, or this computer has no access to it."],
  ["fatal: unable to access 'https://example.com/team/repo.git/': The requested URL returned error: 404", "The repository was not found, or this computer has no access to it."],
  ["fatal: unable to access 'https://github.com/orbyn-dev/private.git/': Could not resolve host: github.com", "Orbyn could not reach the repository. Check the connection, then try again."],
  ["fatal: unable to access 'https://github.com/x/y.git/': Failed to connect to github.com port 443 after 21000 ms: Couldn't connect to server", "Orbyn could not reach the repository. Check the connection, then try again."],
  ["ssh: connect to host github.com port 22: Connection refused", "Orbyn could not reach the repository. Check the connection, then try again."],
])("says %s in one plain sentence", (raw, plain) => {
  expect(shown(raw)).toBe(plain);
});

it("keeps an unknown failure plain, and still says what to try", () => {
  const said = shown("fatal: something only Git knows about");
  expect(said).toBe("Check the address and the connection, then try again.");
  expect(said).not.toContain("something only Git knows");
});
