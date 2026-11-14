// Settings › Crew: the guard's lists (frame `guard-settings`) and the path classes (frames `classes-*`), read-only, through the mock's scenarios.
import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../../mock/api";
import type { Project } from "../../../../api/types";
import { useWorkspace } from "../../../../product/state/workspace";
import { installMockFetch } from "../../../../test-support";
import { CrewSettingsSection } from "./CrewSettings";
import { PathClasses, enginesText } from "./PathClasses";
import { crewApi } from "./api";
import { ProductionTargets } from "./ProductionTargets";

let mock: MockApi;
const calls = (part: string, method = "GET") => vi.mocked(fetch).mock.calls.filter(([url, init]) => String(url).includes(part) && (init?.method ?? "GET") === method);
const frame = (prefix: string) => document.querySelector(`[data-frame^='${prefix}']`)?.getAttribute("data-frame");

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
  useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" } as Project], error: null, loading: false } });
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
    expect(screen.getByText(/Both are this device’s lists, kept on this computer\. They are not in \.orbyn\/crew\.json/)).toBeVisible();
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
  it("reads the rules for the project, with no thread, and lists each class, its paths and the engines allowed, then the built-in class", async () => {
    mock.reset("crew-classes");
    render(<PathClasses />);
    expect(await screen.findByRole("heading", { name: "Crew › Path classes" })).toBeVisible();
    expect(frame("classes-")).toBe("classes-list");
    expect(calls("/api/crew/classes")).toHaveLength(1);
    expect(String(calls("/api/crew/classes")[0]?.[0])).toContain("project=p-app");
    expect(String(calls("/api/crew/classes")[0]?.[0])).not.toContain("thread");
    expect(screen.getByText("Read for the project orbyn-app.")).toBeVisible();
    const table = screen.getByRole("table", { name: "Path classes" });
    expect(within(table).getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Class", "Paths", "Allowed engines"]);
    expect(within(table).getAllByRole("row").slice(1).map((r) => r.textContent)).toEqual([
      "auth-paths**/auth/****/keys/**claude",
      "billing**/billing/**codex",
      "docsdocs/***.mdclaude, codex, opencode",
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

  it("names an engine this machine does not have, and says no engine for a rule that blocks all", () => {
    expect(enginesText([{ id: "cursor", name: "Cursor", on_this_machine: false }])).toBe("cursor (not on this machine)");
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
    expect(detail).toHaveTextContent("Allowed enginescodex");
    expect(detail).toHaveTextContent("ReasonMoney code stays on one vendor.");
    expect(detail).not.toHaveTextContent("routed to");
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
    await user.type(screen.getByRole("textbox", { name: "Paths to test, one per line" }), "auth/redirect.ts");
    const box = await screen.findByText("Logged as");
    expect(box.closest("section")).toHaveTextContent("Pathauth/redirect.ts");
    expect(box.closest("section")).toHaveTextContent("Matchesauth-paths, by **/auth/**");
    expect(box.closest("section")).toHaveTextContent("Allowedclaude");
    expect(box.closest("section")).toHaveTextContent("auth-paths: Sign-in code stays on one vendor. Allowed engines: Claude Code.");
    await waitFor(() => expect(frame("classes-")).toBe("classes-test"));
    // Typing is read once the person stops, as one request with the path as given.
    expect(calls("/api/crew/classes/test")).toHaveLength(1);
    expect(String(calls("/api/crew/classes/test")[0]?.[0])).toContain("path=auth%2Fredirect.ts");
    expect(String(calls("/api/crew/classes/test")[0]?.[0])).toContain("project=p-app&path=auth%2Fredirect.ts");
    expect(String(calls("/api/crew/classes/test")[0]?.[0])).not.toContain("thread");
    expect(calls("/api/crew", "POST")).toHaveLength(0);
  });

  it("with two paths, answers each with the service's own sentence and works out nothing of its own", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.type(screen.getByRole("textbox", { name: "Paths to test, one per line" }), "auth/redirect.ts{Enter}billing/invoice.ts");
    await screen.findByText("billing: Money code stays on one vendor. Allowed engines: Codex.");
    expect(String(calls("/api/crew/classes/test")[0]?.[0])).toContain("path=auth%2Fredirect.ts&path=billing%2Finvoice.ts");
    expect(screen.queryByText("Two paths in one task")).toBeNull();
    expect(screen.queryByText("Allowed by both")).toBeNull();
  });

  it("says what is wrong with a path that is not a repository path, as the service does, and checks nothing else about it", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.type(screen.getByRole("textbox", { name: "Paths to test, one per line" }), "../secrets.txt");
    expect(await screen.findByText("This path has a . or .. part.")).toBeVisible();
    expect(screen.getByText("Not a repository path, so nothing was checked.")).toBeVisible();
  });

  it("sends at most ten paths, the most the service takes", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    await user.click(screen.getByRole("textbox", { name: "Paths to test, one per line" }));
    await user.paste(Array.from({ length: 12 }, (_, i) => `docs/p${i}.md`).join("\n"));
    await waitFor(() => expect(calls("/api/crew/classes/test")).toHaveLength(1));
    expect(new URL(String(calls("/api/crew/classes/test")[0]?.[0]), "http://x").searchParams.getAll("path")).toHaveLength(10);
  });
});

describe("Settings › Crew › Path classes: a command (the guard's test)", () => {
  it("says what the guard says of a command, read once typing stops, and runs nothing", async () => {
    mock.reset("crew-classes");
    const user = userEvent.setup();
    render(<PathClasses />);
    await user.click(await screen.findByRole("button", { name: "Test a path" }));
    expect(screen.getByRole("heading", { name: "Command to test" })).toBeVisible();
    await user.type(screen.getByRole("textbox", { name: "Command to test" }), 'psql "$DATABASE_URL" -c "DROP TABLE x"');
    const box = (await screen.findByText("Matched by")).closest("section");
    expect(box).toHaveTextContent("ResultGuarded: it needs a second key, or you");
    expect(box).toHaveTextContent("Matched bypsql: DROP");
    expect(box).toHaveTextContent("Targetprod-db-1");
    expect(box).toHaveTextContent("Listed inprod_targets: prod-db-1");
    const sent = calls("/api/crew/guard/test");
    expect(sent).toHaveLength(1);
    expect(new URL(String(sent[0]?.[0]), "http://x").searchParams.get("command")).toBe('psql "$DATABASE_URL" -c "DROP TABLE x"');
    expect(calls("/api/crew", "POST")).toHaveLength(0);
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
    expect(calls("/api/crew/classes")).toHaveLength(1);
    await user.click(within(alert).getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(calls("/api/crew/classes")).toHaveLength(2));
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

describe("the classes routes as the service answers them", () => {
  const fails = async (read: Promise<unknown>) => read.then(() => null, (error: { status: number; message: string }) => ({ status: error.status, message: error.message }));

  it("refuses a request with neither thread nor project, an id that is not an id, and an id nothing has", async () => {
    mock.reset("crew-classes");
    expect(await fails(crewApi.classes(""))).toMatchObject({ status: 400, message: "thread or project is required" });
    expect(await fails(crewApi.classes("a b"))).toMatchObject({ status: 400, message: "that id is not valid" });
    expect(await fails(crewApi.classes("p-gone"))).toMatchObject({ status: 404 });
    expect((await crewApi.classes("p-app")).worktree).toBe("feat/crew-map");
  });

  it("takes 1 to 10 paths and says what is wrong with each, as the service does", async () => {
    mock.reset("crew-classes");
    expect(await fails(crewApi.classesTest("p-app", []))).toMatchObject({ status: 400, message: "give 1 to 10 paths" });
    const test = await crewApi.classesTest("p-app", ["C:/x", "a\u0007b", "x".repeat(600), "docs//a", "docs/a.md"]);
    expect(test.paths.map((one) => one.problem)).toEqual([
      "This path starts with a drive letter.",
      "This path has a control character.",
      "This path is longer than 512 characters.",
      "This path has an empty part (a double or trailing /).",
      null,
    ]);
    expect(test.paths[2]?.path).toHaveLength(512);
    expect(test.paths[4]?.route_card).toBe("docs: Anyone may edit the docs. Allowed engines: Claude Code, Codex, OpenCode.");
    expect((await crewApi.classesTest("p-app", ["nothing/here.txt"])).paths[0]?.route_card).toBe("Everything else: any signed-in engine can take this.");
  });

  it("says a route id that is not an id is 400, and one nothing has is 404", async () => {
    mock.reset("crew-route-blocked");
    expect(await fails(crewApi.route("a b"))).toMatchObject({ status: 400 });
    expect(await fails(crewApi.route("task-gone"))).toMatchObject({ status: 404 });
  });
});

describe("Settings › Crew › Path classes: the project", () => {
  it("is the checkout's project when Settings shows one, else the first, and draws nothing with none", async () => {
    mock.reset("crew-classes");
    useWorkspace.setState({ projects: { data: [{ id: "p-app", name: "orbyn-app" }, { id: "p-two", name: "orbyn-two" }] as Project[], error: null, loading: false } });
    window.location.hash = "#/settings/crew?project=g&checkout=p-two";
    const first = render(<PathClasses />);
    expect(await screen.findByText("Read for the project orbyn-two.")).toBeVisible();
    expect(String(calls("/api/crew/classes")[0]?.[0])).toContain("project=p-two");
    first.unmount();
    useWorkspace.setState({ projects: { data: [], error: null, loading: false } });
    const none = render(<PathClasses />);
    expect(none.container).toBeEmptyDOMElement();
  });

  it("says No engine for the built-in class while the file has problems", async () => {
    mock.reset("crew-classes-invalid");
    render(<PathClasses />);
    const table = await screen.findByRole("table", { name: "Path classes" });
    expect(table).toHaveTextContent("Everything elseBuilt inAny path not matched aboveNo engine");
  });
});
