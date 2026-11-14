// Settings › Crew: the guard's lists (frame `guard-settings`) and the path classes (frames `classes-*`), read-only, through the mock's scenarios.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../api/types";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { CrewSettingsSection } from "./CrewSettings";
import { PathClasses, allowedByAll, enginesText } from "./PathClasses";
import { ProductionTargets } from "./ProductionTargets";

let mock: MockApi;
const calls = (part: string, method = "GET") => vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url).includes(part) && (init?.method ?? "GET") === method);
const frame = (prefix: string) => document.querySelector(`[data-frame^='${prefix}']`)?.getAttribute("data-frame");

beforeEach(() => {
  mock = installMockFetch();
  const loadable = <T,>(data: T) => ({ data, error: null, loading: false });
  useWorkspace.setState({ projects: loadable([{ id: "p-app" } as Project]), threadIds: { "p-app": loadable(["t-1"]) } });
});

describe("Settings › Crew › Production-linked targets (guard-settings)", () => {
  it("lists this device's two lists, each entry with what it names and where it is kept", async () => {
    mock.reset("crew");
    render(<ProductionTargets />);
    expect(await screen.findByRole("heading", { name: "Production-linked targets" })).toBeVisible();
    expect(frame("guard-")).toBe("guard-settings");
    const always = screen.getByRole("table", { name: "Always guarded" });
    expect(within(always).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Always guarded", "Matched by", "Source"]);
    expect(within(always).getAllByRole("row").slice(1).map((r) => r.textContent)).toEqual([
      "prod-db-1Remote databasehost prod-db-1prod_targetsThis device’s list",
      "orbyn-assets-prodStorage bucketbucket orbyn-assets-prodprod_targetsThis device’s list",
    ]);
    expect(screen.getByRole("table", { name: "Exempt (safe)" })).toHaveTextContent("staging-db.internalA throwaway staging databasehost staging-db.internalsafe_targetsThis device’s list");
    expect(screen.getByText(/Both are this device’s lists, kept in Settings › Crew on this computer\. They are not in \.orbyn\/crew\.json/)).toBeVisible();
  });

  it("says what counts as destructive by category, what is never approvable, and who the second key is", async () => {
    mock.reset("crew");
    render(<ProductionTargets />);
    await screen.findByRole("heading", { name: "Production-linked targets" });
    const table = screen.getByRole("table", { name: "What counts as destructive" });
    expect(within(table).getAllByRole("row").slice(1).map((r) => r.textContent)).toEqual([
      "DataDROP, TRUNCATE, DELETE without a WHERE",
      "CloudDeleting or terminating a bucket, a volume, an instance or a snapshot with a cloud command",
      "Containersdocker rm or prune against a remote context",
      "Filesrm -r",
      "GitA force push, or a branch delete, to a remote that isn’t local",
    ]);
    expect(screen.getByText(/kubectl delete, terraform destroy and helm uninstall are always denied, in every mode\./)).toBeVisible();
    expect(screen.getByText(/Signed in now: opencode and codex\./)).toBeVisible();
  });

  it("names an empty list and offers no way to change one: adding to a list is not drawn", async () => {
    mock.reset("crew-guard-empty");
    render(<ProductionTargets />);
    await screen.findByRole("heading", { name: "Production-linked targets" });
    expect(screen.getByText("Nothing is listed in prod_targets on this device.")).toBeVisible();
    expect(screen.getByText("Nothing is listed in safe_targets on this device.")).toBeVisible();
    expect(screen.queryByRole("button", { name: /Mark a target/ })).toBeNull();
    expect(calls("/api/crew/guard", "POST")).toHaveLength(0);
  });
});

describe("Settings › Crew › Path classes: the list (classes-list)", () => {
  it("reads the rules at the thread's checkout and lists each class, its paths and the engines allowed, then the built-in class", async () => {
    mock.reset("crew-classes");
    render(<PathClasses />);
    expect(await screen.findByRole("heading", { name: "Crew › Path classes" })).toBeVisible();
    expect(frame("classes-")).toBe("classes-list");
    expect(calls("/api/crew/classes?thread=t-1")).toHaveLength(1);
    const table = screen.getByRole("table", { name: "Path classes" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Class", "Paths", "Allowed engines"]);
    expect(within(table).getAllByRole("row").slice(1).map((r) => r.textContent)).toEqual([
      "auth-paths**/auth/****/keys/**Claude Code",
      "billing**/billing/**Codex",
      "docsdocs/***.mdClaude Code, Codex, OpenCode",
      "Everything elseBuilt inAny path not matched aboveAny signed-in engine",
    ]);
    // Orbyn never writes the file, so nothing here saves, adds or deletes.
    expect(screen.queryByRole("button", { name: /Save|Add|Delete|Open in editor/ })).toBeNull();
    expect(screen.queryByText(/not in force until/)).toBeNull();
  });

  it("says in the service's words when the checkout's copy of the file is not the committed one", async () => {
    mock.reset("crew-classes-note");
    render(<PathClasses />);
    expect(await screen.findByRole("note")).toHaveTextContent("This checkout's copy of the file differs from the committed one. An edit is not in force until it is committed.");
    expect(frame("classes-")).toBe("classes-list");
  });

  it("draws nothing without a thread to read the file for, and asks the service for none", async () => {
    mock.reset("crew-classes");
    useWorkspace.setState({ projects: { data: [], error: null, loading: false }, threadIds: {} });
    const { container } = render(<PathClasses />);
    expect(container).toBeEmptyDOMElement();
    expect(calls("/api/crew/classes")).toHaveLength(0);
  });

  it("names an engine this machine does not have, and says no engine for a rule that blocks all", () => {
    expect(enginesText([{ id: "cursor", name: "Cursor", on_this_machine: false }])).toBe("Cursor (not on this machine)");
    expect(enginesText([])).toBe("No engine");
    expect(enginesText(null)).toBe("Any signed-in engine");
  });
});

describe("Settings › Crew › Path classes: one class (classes-edit)", () => {
  it("opens a read-only detail: where in the file, the paths, the engines and the rule's reason, with no Save", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "billing" }));
    const detail = screen.getByRole("complementary", { name: "The class billing" });
    expect(frame("classes-")).toBe("classes-edit");
    expect(detail).toHaveTextContent("Namebilling");
    expect(detail).toHaveTextContent("WhereRule 2 in .orbyn/crew.json");
    expect(detail).toHaveTextContent("Paths**/billing/**");
    expect(detail).toHaveTextContent("Allowed enginesCodex");
    expect(detail).toHaveTextContent("ReasonMoney code stays on one vendor.");
    expect(detail).toHaveTextContent("Tasks that touch these paths are routed to Codex. A task that also touches another class can use the engines both allow.");
    expect(within(detail).getAllByRole("button").map((b) => b.textContent)).toEqual(["Close"]);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(frame("classes-")).toBe("classes-list");
  });
});

describe("Settings › Crew › Path classes: a path (classes-test)", () => {
  it("shows which class a path matches, by which pattern, and the engines allowed; nothing is saved", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.type(screen.getByRole("textbox", { name: "Test a path" }), "auth/redirect.ts");
    const box = await screen.findByText("Logged as");
    expect(box.closest("section")).toHaveTextContent("Pathauth/redirect.ts");
    expect(box.closest("section")).toHaveTextContent("Matchesauth-paths, by **/auth/**");
    expect(box.closest("section")).toHaveTextContent("AllowedClaude Code");
    expect(box.closest("section")).toHaveTextContent("Allowed by auth-paths: Claude Code.");
    await waitFor(() => expect(frame("classes-")).toBe("classes-test"));
    // Typing is read once the person stops, as one request with the path as given.
    expect(calls("/api/crew/classes/test")).toHaveLength(1);
    expect(String(calls("/api/crew/classes/test")[0]?.[0])).toContain("thread=t-1&path=auth%2Fredirect.ts");
    expect(calls("/api/crew", "POST")).toHaveLength(0);
  });

  it("with two paths that no engine may take together, says no engine", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.type(screen.getByRole("textbox", { name: "Test a path" }), "auth/redirect.ts, billing/invoice.ts");
    await screen.findByText("Two paths in one task");
    const test = String(calls("/api/crew/classes/test")[0]?.[0]);
    expect(test).toContain("path=auth%2Fredirect.ts&path=billing%2Finvoice.ts");
    const together = screen.getByText("Two paths in one task").closest("section");
    expect(together).toHaveTextContent("Matchesauth-paths and billing");
    expect(together).toHaveTextContent("Allowed by bothNo engine");
  });

  it("says what is wrong with a path that is not a repository path, as the service does, and checks nothing else about it", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.type(screen.getByRole("textbox", { name: "Test a path" }), "../secrets.txt");
    expect(await screen.findByText("This path has an empty, . or .. part.")).toBeVisible();
    expect(screen.getByText("Not a repository path, so nothing was checked.")).toBeVisible();
  });

  it("sends at most ten paths, the most the service takes", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.click(screen.getByRole("textbox", { name: "Test a path" }));
    await user.paste(Array.from({ length: 12 }, (_, i) => `docs/p${i}.md`).join(","));
    await waitFor(() => expect(calls("/api/crew/classes/test")).toHaveLength(1));
    expect(new URL(String(calls("/api/crew/classes/test")[0]?.[0]), "http://x").searchParams.getAll("path")).toHaveLength(10);
  });

  it("allows what every path's list allows, and any engine for a path with no rule", () => {
    const one = (ids: string[] | null) => ({ path: "x", valid: true, problem: null, classes: [], allowed_engines: ids && ids.map((id) => ({ id, name: id, on_this_machine: true })), route_card: "" });
    expect(allowedByAll([one(["a", "b"]), one(["b", "c"])])?.map((e) => e.id)).toEqual(["b"]);
    expect(allowedByAll([one(["a"]), one(null)])?.map((e) => e.id)).toEqual(["a"]);
    expect(allowedByAll([one(null)])).toBeNull();
    expect(allowedByAll([one(["a"]), one(["b"])])).toEqual([]);
  });
});

describe("Settings › Crew › Path classes: what is wrong in the file (classes-error)", () => {
  it("says it can't use the file, lists each problem in the service's words, and reads the file again on Check again", async () => {
    mock.reset("crew-classes-invalid");
    const user = userEvent.setup();
    render(<PathClasses />);
    const alert = await screen.findByRole("alert");
    expect(frame("classes-")).toBe("classes-error");
    expect(alert).toHaveTextContent("Orbyn can’t use .orbyn/crew.json yetIt found 3 problems. Until they are fixed, every engine is blocked by the rule “crew.json invalid”, nothing starts, and a task that needs routing waits in Needs you.");
    const problems = screen.getByRole("table", { name: "What is wrong in the file" });
    expect(within(problems).getAllByRole("row").slice(1).map((r) => r.textContent)).toEqual([
      "Rule 4 (keys): the path pattern **/keys/[ isn’t valid: the bracket isn’t closed.",
      "Rule 4 (keys): no engine is chosen.",
      "Rule 5 (billing): the name billing is already used by rule 2.",
    ]);
    expect(calls("/api/crew/classes?")).toHaveLength(1);
    await user.click(within(alert).getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(calls("/api/crew/classes?")).toHaveLength(2));
  });
});

describe("Settings › Crew", () => {
  it("shows Autopilot, the guard's lists and the path classes in one page", async () => {
    mock.reset("crew-classes");
    render(<CrewSettingsSection />);
    expect(await screen.findByRole("heading", { name: "Autopilot" })).toBeVisible();
    expect(await screen.findByRole("heading", { name: "Production-linked targets" })).toBeVisible();
    expect(await screen.findByRole("heading", { name: "Crew › Path classes" })).toBeVisible();
  });
});
