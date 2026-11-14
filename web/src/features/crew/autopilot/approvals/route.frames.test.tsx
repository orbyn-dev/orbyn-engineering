// The Route card where a path class decided (frames `route-class-*`) and the Approval log's Routing view (frames `route-log*`), read through the
// mock's `crew-route-*` scenarios: what `GET /api/crew/route/{task}` and the ledger's `route_class` rows say.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { CrewRouteView } from "../../../../generated/CrewRouteView";
import { installMockFetch } from "../../../../test-support";
import { inApp } from "./test-support";
import { ApprovalsTab } from "./ApprovalsTab";
import { RouteCard, RouteClassCard, routeFrame } from "./RouteCard";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});
const calls = (part: string) => vi.mocked(fetch).mock.calls.filter(([url]) => String(url).includes(part));
const frame = (prefix: string) => document.querySelector(`[data-frame^='${prefix}']`)?.getAttribute("data-frame");

describe("the Route card, an engine blocked by a path class (route-class-blocked)", () => {
  it("names the engine chosen and every engine a rule blocked, with the rule's own words and its name", async () => {
    mock.reset("crew-route-blocked");
    render(<RouteCard task="task-redirect" />);
    const card = await screen.findByRole("region", { name: "Route" });
    expect(frame("route-class-")).toBe("route-class-blocked");
    expect(card).toHaveTextContent("Why claude was picked");
    expect(card).toHaveTextContent("Chosen: claude · Sonnet 5.5 · Medium");
    expect(card).toHaveTextContent("The rule auth-paths allows claude for the paths this task touches, and it is signed in.");
    expect(card).toHaveTextContent("Touchesauth/redirect.ts, auth/redirect.test.ts, auth/session.ts");
    expect(card).toHaveTextContent("Path classauth-paths");
    const rows = within(card).getAllByRole("listitem");
    expect(rows.map((r) => r.textContent)).toEqual([
      "claude · Sonnet 5.5Chosen",
      "codexBlocked by rule: auth-paths, allowed engines: Claude. Sign-in code stays on one vendor.auth-paths",
      "opencodeBlocked by rule: auth-paths, allowed engines: Claude. Sign-in code stays on one vendor.auth-paths",
      "cursorNot signed in. Sign in",
    ]);
    // The tag says where the rule lives, and a blocked engine has no way to be picked here.
    expect(within(rows[1] as HTMLElement).getByText("auth-paths")).toHaveAttribute("title", "The rule auth-paths in .orbyn/crew.json");
    expect(screen.queryByRole("button", { name: /Pin|Open the rule/ })).toBeNull();
  });

  it("sends Sign in for an engine that is not signed in to Providers", async () => {
    mock.reset("crew-route-blocked");
    const user = userEvent.setup();
    render(<RouteCard task="task-redirect" />);
    await user.click(await screen.findByRole("button", { name: "Sign in" }));
    expect(window.location.hash).toBe("#/settings/providers");
  });
});

describe("the Route card, the file isn't valid (route-class-invalid)", () => {
  it("says nothing started, shows the problem in the service's words, and reads the file again on Check again", async () => {
    mock.reset("crew-route-invalid");
    const user = userEvent.setup();
    render(<RouteCard task="task-redirect" />);
    const card = await screen.findByRole("region", { name: "Route" });
    expect(frame("route-class-")).toBe("route-class-invalid");
    expect(card).toHaveTextContent("Why no engine was pickedNeeds you");
    expect(card).toHaveTextContent(".orbyn/crew.json isn’t valid, so no engine was started");
    expect(card).toHaveTextContent("File.orbyn/crew.jsonProblemRule 3 has no engines listed.");
    expect(card).toHaveTextContent("Read14:20");
    expect(card).toHaveTextContent("It can’t tell which engines the rules allow, so it blocks every engine until the file is valid.");
    const tags = within(card).getAllByText("crew.json invalid");
    expect(tags).toHaveLength(3);
    expect(card).toHaveTextContent("claudeBlocked by rule: the file isn’t validcrew.json invalid");
    expect(screen.queryByRole("button", { name: "Open in editor" })).toBeNull();
    expect(calls("/api/crew/route/task-redirect")).toHaveLength(1);
    await user.click(within(card).getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(calls("/api/crew/route/task-redirect")).toHaveLength(2));
  });
});

describe("the Route card, the rules exclude every engine (route-class-none)", () => {
  it("says no engine is allowed, what the task touched, and each engine with the rule that blocked it", async () => {
    mock.reset("crew-route-none");
    render(<RouteCard task="task-redirect" />);
    const card = await screen.findByRole("region", { name: "Route" });
    expect(frame("route-class-")).toBe("route-class-none");
    expect(card).toHaveTextContent("Why no engine was pickedNeeds you");
    expect(card).toHaveTextContent("No engine is allowed for these paths");
    expect(card).toHaveTextContent("found 2 path classes, and no engine is allowed by both rules. Nothing was started.");
    expect(card).toHaveTextContent("Touchesauth/redirect.ts, billing/invoice.ts");
    expect(card).toHaveTextContent("Path classesauth-paths and billing");
    expect(within(card).getAllByRole("listitem").map((r) => r.textContent)).toEqual([
      "claudeBlocked by rule: billing — allowed engines: Codexbilling",
      "codexBlocked by rule: auth-paths — allowed engines: Claudeauth-paths",
      "opencodeBlocked by both rules: auth-paths and billingauth-pathsbilling",
      "cursorNot signed in. Sign in",
    ]);
    // Splitting the task, changing the rule and asking to be asked are answers the service cannot take yet: none is drawn.
    expect(screen.queryByRole("button", { name: /Split the task|Change the rule|Ask me to choose/ })).toBeNull();
  });
});

describe("the Route card where no rule took part", () => {
  const view = { task: "t", state: "running", rows: [] } as CrewRouteView;
  it("is not drawn: no frame, no card", () => {
    expect(routeFrame(view)).toBeNull();
    const { container } = render(<RouteClassCard view={view} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("says it can't read the route of a task the service does not know", async () => {
    mock.reset("crew-route-blocked");
    render(<RouteCard task="task-gone" />);
    expect(await screen.findByText("Can't read the route")).toBeVisible();
  });
});

async function openLog(scenario: "crew" | `crew-${string}`) {
  mock.reset(scenario);
  await inApp(true, vi.fn());
  const user = userEvent.setup();
  render(<ApprovalsTab />);
  await screen.findByRole("region", { name: "Approvals" });
  await screen.findByRole("table", { name: "Approval log" });
  return user;
}

describe("the Approval log's Routing view (route-log, route-log-none)", () => {
  it("is offered only once a routing decision is in the ledger", async () => {
    await openLog("crew");
    expect(screen.queryByRole("group", { name: "Show" })).toBeNull();
  });

  it("lists a routing with no engine allowed in amber, with the version of crew.json that made it (route-log-none)", async () => {
    const user = await openLog("crew-route-none");
    const filter = screen.getByRole("group", { name: "Show" });
    expect(filter).toHaveTextContent("Requests");
    expect(within(filter).getByRole("button", { name: /Routing/ })).toHaveTextContent("Routing 1");
    await user.click(within(filter).getByRole("button", { name: /Routing/ }));
    expect(screen.getByText("Routing is decided by code from .orbyn/crew.json, before any engine is scored")).toBeVisible();
    expect(frame("route-log")).toBe("route-log-none");
    const table = screen.getByRole("table", { name: "Routing decisions" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Time", "Task, class and sample paths", "Blocked by a rule", "Chosen", "crew.json", "Result"]);
    const row = within(table).getAllByRole("row")[1] as HTMLElement;
    expect(row).toHaveTextContent("task-redirect");
    expect(row).toHaveTextContent("Class auth-paths and billing · **/auth/**, **/billing/**");
    expect(row).toHaveTextContent("auth/redirect.tsbilling/invoice.ts");
    expect(row).toHaveTextContent("claude · billing");
    expect(row).toHaveTextContent("opencode · auth-paths");
    expect(row).toHaveTextContent("No engineWaiting in Needs you");
    expect(row).toHaveTextContent("a1c4e7dblob 5be90f2");
    expect(row).toHaveTextContent("No engine allowed");
    expect(row).not.toHaveTextContent("Routed");
  });

  it("lists a routed task with the engine chosen, three sample paths and the class that allowed it (route-log)", async () => {
    const user = await openLog("crew-route-blocked");
    await user.click(screen.getByRole("button", { name: /Routing/ }));
    expect(frame("route-log")).toBe("route-log");
    const row = within(screen.getByRole("table", { name: "Routing decisions" })).getAllByRole("row")[1] as HTMLElement;
    expect(row).toHaveTextContent("auth/redirect.tsauth/redirect.test.tsauth/session.ts");
    expect(row).not.toHaveTextContent("more");
    expect(row).toHaveTextContent("claudeAllowed by auth-paths");
    expect(row).toHaveTextContent("Routed");
    // The ledger is read for the routing rows by their kind, and the requests view comes back with its filter.
    expect(calls("kind=route_class").length).toBeGreaterThan(0);
    await user.click(screen.getByRole("button", { name: /Requests/ }));
    expect(screen.getByRole("table", { name: "Approval log" })).toBeVisible();
  });
});
