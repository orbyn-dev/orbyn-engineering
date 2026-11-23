import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import type { EngineStatus, Project, Thread } from "../../api/types";
import { useUi } from "../../product/state/ui";
import { useWorkspace } from "../../product/state/workspace";
import { setIssuesApi } from "../issues/api";
import { WelcomePage } from "./WelcomePage";

const writes: unknown[] = [];
let failWrite = false;
vi.mock("../../api/client", async (original) => {
  const actual = await original<typeof import("../../api/client")>();
  return {
    ...actual,
    request: vi.fn(async (method: string, path: string, body?: unknown) => {
      if (path === "/api/settings/preferences/query")
        return {
          catalog: { version: "v1" },
          rows: ["default_engine", "default_model", "default_effort", "default_model_options"].map((key) => ({ key, value: key === "default_model_options" ? {} : "", revision: `r-${key}` })),
        };
      if (path === "/api/settings/preferences" && method === "PUT") {
        if (failWrite) throw new actual.ApiFailure("unavailable", "The service did not answer.", 503);
        writes.push(body);
        return { catalog: { version: "v2" }, rows: [] };
      }
      // The Issues flag is off: the real client reads this 404 as "this service reports no issues".
      if (path === "/api/issues/trackers") throw new actual.ApiFailure("not_found", "Not found.", 404);
      return [];
    }),
  };
});

const engine = (patch: Partial<EngineStatus>) =>
  ({ id: "claude", name: "Claude Code", tier: "managed", installed: true, auth: "logged_in", version: "2.1.318", sign_in_command: "claude login", ...patch }) as EngineStatus;
const project = (id: string, name: string, over: Partial<Project> = {}) =>
  ({ id, name, path: `~/code/${id}`, trusted: true, is_git: true, created_at: "2026-01-01T00:00:00Z", managed: false, scratch: false, ...over }) as Project;
const thread = (id: string, project_id: string, updated_at: string) => ({ id, project_id, updated_at, lifecycle: { archived_at: null } }) as unknown as Thread;

function load(engines: EngineStatus[] | null, projects: Project[] = [], threads: Thread[] = []) {
  useWorkspace.setState({
    engines: { data: engines, loading: false, error: null },
    projects: { data: projects, loading: false, error: null },
    threads: Object.fromEntries(threads.map((entry) => [entry.id, entry])),
    managed: { data: null, loading: false, error: null },
    refreshEngines: vi.fn(async () => undefined),
  } as never);
}

beforeEach(() => {
  writes.length = 0;
  failWrite = false;
  useUi.setState({ toasts: [] });
  window.location.hash = "";
  useUi.getState().setPalette(null);
});
afterEach(() => {
  setIssuesApi(null);
  vi.restoreAllMocks();
});

describe("Welcome page", () => {
  it("opens the add-project flows from its three Projects actions, one click each", async () => {
    load([engine({})]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    await user.click(screen.getByRole("button", { name: "New project" }));
    expect(useUi.getState().palette).toBe("add-project-new");
    await user.click(screen.getByRole("button", { name: "Clone a repository" }));
    expect(useUi.getState().palette).toBe("add-project-url");
    await user.click(screen.getByRole("button", { name: "Add a folder" }));
    expect(useUi.getState().palette).toBe("add-project");
  });

  it("reaches New project by keyboard, and Esc closes the flow it opened", async () => {
    load([engine({})]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    const button = screen.getByRole("button", { name: "New project" });
    button.focus();
    await user.keyboard("{Enter}");
    expect(useUi.getState().palette).toBe("add-project-new");
    act(() => useUi.getState().setPalette(null));
    expect(useUi.getState().palette).toBeNull();
  });

  it("draws Projects as the section that holds the actions and the recent list, in the design's order", () => {
    load([engine({})], [project("a", "orbyn-engineering")]);
    render(<WelcomePage />);
    const section = screen.getByRole("region", { name: "Projects" });
    const buttons = within(section).getAllByRole("button").map((b) => b.textContent);
    expect(buttons).toEqual(["New project", "Clone a repository", "Add a folder"]);
    expect(within(section).getByRole("heading", { name: "Recent projects" })).toBeVisible();
  });

  it("says where projects will appear on a first run", () => {
    load([engine({})]);
    render(<WelcomePage />);
    expect(screen.getByText("Projects you open or clone show up here.")).toBeInTheDocument();
  });

  it("lists recent projects, newest first, with their threads, and a row opens the project", async () => {
    load([engine({})], [project("a", "orbyn-engineering"), project("b", "orbyn-planner")], [thread("1", "a", "2026-11-21T10:00:00Z"), thread("2", "a", "2026-11-21T09:00:00Z"), thread("3", "b", "2026-11-01T09:00:00Z")]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    const rows = screen.getAllByRole("link");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("orbyn-engineering");
    expect(rows[0]).toHaveTextContent("2 threads");
    expect(rows[1]).toHaveTextContent("1 thread");
    await user.click(rows[1]!);
    expect(window.location.hash).toBe("#/new/b");
  });

  it("sends an untrusted project to its trust page", async () => {
    load([engine({})], [project("c", "fresh", { trusted: false })]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    await user.click(screen.getByRole("link"));
    expect(window.location.hash).toBe("#/trust/c");
  });

  it("draws a hostile project name and path as text", () => {
    load([engine({})], [project("x", "<img src=x onerror=alert(1)>", { path: "<script>alert(1)</script>" })]);
    const { container } = render(<WelcomePage />);
    expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeInTheDocument();
    expect(screen.getByText("<script>alert(1)</script>")).toBeInTheDocument();
    expect(container.querySelector("img, script")).toBeNull();
  });
});

describe("agents", () => {
  it("shows each agent with its status and its one next step", () => {
    load([engine({}), engine({ id: "codex", name: "Codex", auth: "logged_out", version: null }), engine({ id: "oc", name: "OpenCode", installed: false, version: null })]);
    render(<WelcomePage />);
    const rows = within(screen.getByRole("list", { name: "Agents on this computer" })).getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent("2.1.318 · signed in");
    expect(within(rows[0]!).getByRole("button", { name: "Use for new threads" })).toBeInTheDocument();
    expect(rows[1]).toHaveTextContent("Needs sign-in");
    expect(within(rows[1]!).getByRole("button", { name: "Sign in" })).toBeInTheDocument();
    expect(rows[2]).toHaveTextContent("Not found");
    expect(within(rows[2]!).getByRole("link", { name: "How to install" })).toHaveAttribute("href", "#/settings/providers");
  });

  it("says no engine is ready, and Check again reads the engines once", async () => {
    load([engine({ installed: false }), engine({ id: "codex", name: "Codex", auth: "logged_out" })]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    expect(screen.getByText(/No engine is ready/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Check again" }));
    expect(useWorkspace.getState().refreshEngines).toHaveBeenCalledTimes(1);
  });

  it("has no banner while an engine is ready", () => {
    load([engine({})]);
    render(<WelcomePage />);
    expect(screen.queryByText(/No engine is ready/)).toBeNull();
  });

  it("makes no write until Use for new threads is clicked, then one write that sets the engine and its automatic model", async () => {
    load([engine({}), engine({ id: "codex", name: "Codex" })]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Use for new threads" })[0]).toBeEnabled());
    expect(writes).toHaveLength(0);
    await user.click(screen.getAllByRole("button", { name: "Use for new threads" })[1]!);
    await waitFor(() => expect(writes).toHaveLength(1));
    expect(writes[0]).toMatchObject({ scope: {}, membership_version: "v1", edits: { default_engine: { op: "set", value: "codex" }, default_model: { op: "set", value: "" } } });
  });

  it("names the section Agents and says in a toast which engine new threads now start on", async () => {
    load([engine({}), engine({ id: "codex", name: "Codex" })]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    expect(screen.getByRole("region", { name: "Agents" })).toBeVisible();
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Use for new threads" })[1]).toBeEnabled());
    await user.click(screen.getAllByRole("button", { name: "Use for new threads" })[1]!);
    await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["New threads start on Codex."]));
  });

  it("says the engine could not be changed with the reason, keeps the choice where it was and leaves the button pressable again", async () => {
    load([engine({}), engine({ id: "codex", name: "Codex" })]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Use for new threads" })[1]).toBeEnabled());
    failWrite = true;
    await user.click(screen.getAllByRole("button", { name: "Use for new threads" })[1]!);
    await waitFor(() => expect(useUi.getState().toasts.map((toast) => toast.text)).toEqual(["Couldn't change the engine. The service did not answer."]));
    await waitFor(() => expect(screen.getAllByRole("button", { name: "Use for new threads" })).toHaveLength(2));
    expect(screen.getAllByRole("button", { name: "Use for new threads" })[1]).toBeEnabled();
    expect(writes).toHaveLength(0);
  });

  it("copies the sign-in command on a click and tells the person to run it", async () => {
    load([engine({ auth: "logged_out" })]);
    const user = userEvent.setup();
    const writeText = vi.spyOn(navigator.clipboard, "writeText");
    render(<WelcomePage />);
    await user.click(screen.getByRole("button", { name: "Sign in" }));
    expect(writeText).toHaveBeenCalledWith("claude login");
  });
});

describe("Issue trackers section", () => {
  it("is not drawn while the service answers 404 to the trackers", async () => {
    load([engine({})]);
    render(<WelcomePage />);
    await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
    expect(screen.queryByRole("heading", { name: /Issue trackers/ })).toBeNull();
    expect(screen.getByText(/pick the engine/)).not.toHaveTextContent(/tracker/);
  });

  it("lists the five trackers with their states; Add token and Replace go to Settings with no field and no write", async () => {
    setIssuesApi(createIssuesMock({
      trackers: [
        { tracker: "github", connected: true },
        { tracker: "gitlab", connected: true, host: "https://gitlab.com" },
        { tracker: "azure_devops", connected: false, host: "https://dev.azure.com/orbyn", detail: "Azure DevOps did not accept the token." },
        { tracker: "forgejo", connected: false },
        { tracker: "linear", connected: true, scope: "Orbyn team" },
      ],
    }));
    load([engine({})]);
    const user = userEvent.setup();
    render(<WelcomePage />);
    const list = await screen.findByRole("list", { name: "Issue trackers" });
    const rows = within(list).getAllByRole("listitem");
    expect(rows.map((row) => row.querySelector("b")?.textContent)).toEqual(["GitHub", "GitLab", "Azure DevOps", "Forgejo", "Linear"]);
    expect(rows[0]).toHaveTextContent("Uses the gh sign-in on this computer");
    expect(rows[0]).toHaveTextContent("Signed in");
    expect(rows[1]).toHaveTextContent("Saved in your keychain · https://gitlab.com");
    expect(rows[2]).toHaveTextContent("Needs attention");
    expect(screen.getByRole("alert")).toHaveTextContent("Azure DevOps needs attention: Azure DevOps did not accept the token. Replace the token or remove it in Settings › Source control.");
    expect(rows[3]).toHaveTextContent("Not connected");
    expect(screen.getByText("Issues appear in the left bar once a tracker is connected.")).toBeVisible();
    expect(document.querySelector("input[type=password]")).toBeNull();
    await user.click(within(rows[3]!).getByRole("button", { name: "Add a token for Forgejo" }));
    expect(window.location.hash).toBe("#/settings/source-control");
    window.location.hash = "";
    await user.click(within(rows[1]!).getByRole("button", { name: "Replace the token for GitLab" }));
    expect(window.location.hash).toBe("#/settings/source-control");
    expect(within(rows[0]!).queryByRole("button")).toBeNull();
    expect(writes).toHaveLength(0);
  });
});
