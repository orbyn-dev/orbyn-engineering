/**
 * A test file that renders the whole app imports this beside `App`.
 *
 * The route screens are their own chunks (app/shell/routes.tsx), and Vite transforms a feature's
 * module graph the first time someone imports it: seconds, inside the test that first opens that
 * screen, where it used to sit in the file's import of `App`, which imported every screen. Reading
 * them here keeps that wait in the import, out of a test's timeouts, without changing what the test
 * renders. Files that do not render the app import nothing of the sort: a feature module read before
 * a test has set up its own environment can lock in that environment.
 */
await Promise.all([
  import("../features/onboarding"),
  import("../features/issues"),
  import("../features/pulls"),
  import("../features/settings"),
  import("../features/skills"),
  import("../features/usage"),
]);

export {};
