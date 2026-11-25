import { describe, expect, it } from "vitest";
import { NO_SUCH_ROUTE, routeAbsent } from "./edit";

/**
 * Whether an answer means "this service has no such route" (docs/issues.md, "Issue writes and list controls"). The two `404`s that
 * matter are different facts and are read apart in both directions (#1530): the SPA fallback answers a path no route claimed with
 * `404 not_found` and its own sentence (`crates/orbynd/src/routes/ui.rs`, pinned by
 * `crates/orbynd/src/routes/tests.rs`'s `a_service_prefix_miss_answers_no_such_route`), while a route that exists and cannot find
 * the issue it was asked for answers `404 not_found` with a sentence naming what it could not find.
 */
describe("a route that is not there, and a thing that is not there", () => {
  it("reads the SPA fallback's own 404 not_found as no such route (#1530)", () => {
    // `NO_SUCH_ROUTE` is the web's one copy of the sentence; the service's copy is pinned by the orbynd test named there.
    expect(routeAbsent(404, "not_found", NO_SUCH_ROUTE)).toBe(true);
  });

  it("keeps a route's own not_found for a missing issue as an answer, not as an absent route", () => {
    // A 404 the API itself wrote about the issue it was asked for: the route is there, and this is its answer.
    expect(routeAbsent(404, "not_found", "issue not found")).toBe(false);
    expect(routeAbsent(404, "not_found", "No issue #7 in orbyn-dev/orbyn-engineering on GitHub.")).toBe(false);
    expect(routeAbsent(404, "not_found", "No such repository.")).toBe(false);
    // An answer the client could not read says nothing about the route either: it is not evidence of absence.
    expect(routeAbsent(404, "not_found", "The server answered 404.")).toBe(false);
    expect(routeAbsent(404, "not_found", "")).toBe(false);
  });

  it("still reads a 405, and a 404 whose code is not not_found, as no such route", () => {
    expect(routeAbsent(405, "method_not_allowed", "No.")).toBe(true);
    expect(routeAbsent(404, "no_ui", "No UI is installed.")).toBe(true);
    expect(routeAbsent(404, "http_404", "The server answered 404.")).toBe(true);
  });

  it("reads every answer that is not a 404 or 405 as the route being there", () => {
    for (const [status, code] of [[200, ""], [204, ""], [400, "invalid"], [409, "tracker_not_connected"], [403, "person_required"], [500, "internal"]] as const) {
      expect(routeAbsent(status, code, NO_SUCH_ROUTE), `${status} ${code}`).toBe(false);
    }
  });
});
