import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { MockApi } from "../../../../mock/api";
import type { ScopedPreferencesResult } from "../../../generated/ScopedPreferencesResult";
import { installMockFetch } from "../../../test-support";
import { SettingsScopeProvider } from "../rows/SettingsScopeContext";
import { preferenceWrites } from "../rows/preferenceWrites.test-support";
import { TextGenerators } from "./TextGenerators";

const KEY = "text_generation_models";
/** The checkout the project-scoped tests read and write: the mock service's first project. */
const PROJECT = "p-app";
const PROJECT_GROUP = `mock:${PROJECT}`;
const PROJECT_HASH = `#/settings/threads?project=${PROJECT_GROUP}&checkout=${PROJECT}`;

let mock: MockApi;
const rowOf = (title: string) => screen.getByText(title).closest(".setting-line") as HTMLElement;

/** Saves the per-engine map the service stores, at the device or for one project. */
const store = (map: Record<string, string>, project?: string) =>
  mock.handle("PUT", project ? `/api/projects/${project}/preferences` : "/api/preferences", { values: { [KEY]: map } });
/** The map the service now stores, at the device or for one project. */
const stored = (project?: string) => {
  const scope = project ? { selection: { kind: "checkout", group_key: `mock:${project}`, project_id: project } } : {};
  const read = mock.handle("POST", "/api/settings/preferences/query", { scope, keys: [KEY] }).body as ScopedPreferencesResult;
  const row = read.rows[0];
  return (project ? row?.members[0]?.view.value : row?.value) as Record<string, string>;
};
const wireCalls = (method: string, startsWith: string) =>
  vi.mocked(fetch).mock.calls.filter(([url, init]) => (init?.method ?? "GET") === method && String(url).startsWith(startsWith));
const listReads = () => wireCalls("GET", "/api/text-generators").length;

beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

afterEach(() => vi.useRealTimers());

describe("Settings › Threads › Text generators", () => {
  it("draws one row per engine with the frame's strings and what Automatic resolves to", async () => {
    mock.handle("PUT", "/api/preferences", { values: { text_generation_model: "haiku-5-5", disabled_engines: ["kimi"] } });
    render(<TextGenerators />);
    expect(await screen.findByRole("heading", { name: "Text generators" })).toBeVisible();
    expect(screen.getByText("One model per engine writes a thread’s title and the text of a commit or pull request you confirm. Automatic uses the engine’s small model.")).toBeVisible();

    const claudeRow = rowOf("Claude Code");
    expect(within(claudeRow).getByText("Haiku 5.5 · from Title model")).toBeVisible();
    expect(within(claudeRow).getByRole("button", { name: "Automatic" })).toBeVisible();

    const codexRow = rowOf("Codex");
    expect(within(codexRow).getByText("GPT-5.2 mini · the engine’s small model")).toBeVisible();
    expect(within(codexRow).getByRole("button", { name: "Automatic" })).toBeVisible();

    const cursorRow = rowOf("Cursor");
    expect(within(cursorRow).getByText("No small model listed · the thread’s own model writes")).toBeVisible();
    expect(within(cursorRow).getByRole("button", { name: "Automatic" })).toBeVisible();
    expect(screen.queryByLabelText("Set for this project")).not.toBeInTheDocument();
  });

  it("shows a saved choice as Your choice, ticks it, tags Default and Added in settings, and leaves Automatic selectable", async () => {
    const user = userEvent.setup();
    store({ codex: "gpt-5.2-codex" });
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    expect(within(codexRow).getByText("Your choice")).toBeVisible();
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" }));

    const automatic = screen.getByRole("menuitem", { name: "Automatic" });
    expect(automatic).toBeEnabled();
    expect(automatic).not.toHaveAttribute("title");
    expect(screen.getByRole("menuitem", { name: "GPT-5.2 Codex" })).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("menuitem", { name: "GPT-5.2 Codex Max" })).toHaveTextContent("Default");
    expect(screen.getByRole("menuitem", { name: "team-tuned-mini" })).toHaveTextContent("Added in settings");
  });

  it("sends the full stored map when choosing a model, keeping engines the list leaves out", async () => {
    const user = userEvent.setup();
    store({ claude: "haiku-5-5", ghost: "ghost-model" });
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    await user.click(within(codexRow).getByRole("button", { name: "Automatic" }));
    await user.click(screen.getByRole("menuitem", { name: "GPT-5.2 Codex" }));

    await waitFor(() => expect(preferenceWrites()).toHaveLength(1));
    const [write] = preferenceWrites();
    expect(write?.scope).toEqual({});
    expect(Object.keys(write?.if_match ?? {})).toEqual([KEY]);
    expect(write?.edits).toEqual({ [KEY]: { op: "set", value: { claude: "haiku-5-5", ghost: "ghost-model", codex: "gpt-5.2-codex" } } });
    expect(stored()).toEqual({ claude: "haiku-5-5", ghost: "ghost-model", codex: "gpt-5.2-codex" });
    expect(within(codexRow).getByText("Your choice")).toBeVisible();
  });

  it("returns one engine to Automatic with one write that removes only its entry", async () => {
    const user = userEvent.setup();
    store({ codex: "gpt-5.2-codex", claude: "haiku-5-5", ghost: "ghost-model" });
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" }));
    await user.click(screen.getByRole("menuitem", { name: "Automatic" }));

    await waitFor(() => expect(preferenceWrites()).toHaveLength(1));
    expect(preferenceWrites()[0]?.edits).toEqual({ [KEY]: { op: "set", value: { claude: "haiku-5-5", ghost: "ghost-model" } } });
    expect(stored()).toEqual({ claude: "haiku-5-5", ghost: "ghost-model" });
  });

  it("does nothing when Automatic is picked while nothing is chosen", async () => {
    const user = userEvent.setup();
    render(<TextGenerators />);
    await screen.findByText("Codex");
    await user.click(within(rowOf("Codex")).getByRole("button", { name: "Automatic" }));
    await user.click(screen.getByRole("menuitem", { name: "Automatic" }));

    expect(vi.mocked(fetch).mock.calls.filter(([, init]) => (init?.method ?? "GET") !== "GET")).toEqual([]);
    expect(stored()).toEqual({});
  });

  it("shows the service's refusal under that row, keeps its saved choice, and reloads the list", async () => {
    const user = userEvent.setup();
    store({ codex: "gpt-5.2-codex" });
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" }));
    mock.fail("/api/settings/preferences", 1, 400, "invalid_input", "Codex does not list gpt-5.2-codex-max as a model.", "PUT");
    await user.click(screen.getByRole("menuitem", { name: "GPT-5.2 Codex Max" }));

    expect(await within(codexRow).findByRole("alert")).toHaveTextContent("Codex does not list gpt-5.2-codex-max as a model.");
    expect(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" })).toBeVisible();
    expect(within(codexRow).queryByRole("button", { name: "GPT-5.2 Codex Max" })).toBeNull();
    await waitFor(() => expect(listReads()).toBe(2));
    expect(stored()).toEqual({ codex: "gpt-5.2-codex" });
  });

  it("waits 400 ms before drawing skeleton rows", async () => {
    vi.useFakeTimers();
    // The network never answers.
    vi.mocked(fetch).mockImplementation(() => new Promise(() => undefined));
    render(<TextGenerators />);
    expect(screen.getByRole("heading", { name: "Text generators" })).toBeVisible();
    expect(document.querySelectorAll(".skeleton")).toHaveLength(0);
    act(() => vi.advanceTimersByTime(399));
    expect(document.querySelectorAll(".skeleton")).toHaveLength(0);
    act(() => vi.advanceTimersByTime(1));
    expect(document.querySelectorAll(".skeleton")).toHaveLength(3);
  });

  it("draws the page's error state with the frame title and retries", async () => {
    const user = userEvent.setup();
    // The service cannot be reached for the first read.
    vi.mocked(fetch).mockRejectedValueOnce(new TypeError("Failed to fetch"));
    render(<TextGenerators />);
    expect(await screen.findByText("Couldn’t load text generators")).toBeVisible();
    expect(screen.getByText("Check that the Orbyn service is running.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Claude Code")).toBeVisible();
  });

  it("draws the empty line and opens Providers", async () => {
    const user = userEvent.setup();
    mock.handle("PUT", "/api/preferences", { values: { disabled_engines: ["claude", "codex", "opencode", "cursor"] } });
    window.location.hash = "#/settings/threads";
    render(<TextGenerators />);
    expect(await screen.findByText("No engine is on for this project")).toBeVisible();
    expect(screen.getByText("Titles and commit text need one. Turn an engine on in Providers.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Open Providers" }));
    expect(window.location.hash).toBe("#/settings/providers");
  });

  it("turns the control off for an engine that listed no models", async () => {
    render(<TextGenerators />);
    await screen.findByText("OpenCode");
    const row = rowOf("OpenCode");
    expect(within(row).getByText("The engine returned no model list · the thread’s own model writes")).toBeVisible();
    expect(within(row).getByRole("button", { name: "No models listed" })).toBeDisabled();
  });

  it("reads and writes the project's own map when the settings scope is a checkout", async () => {
    const user = userEvent.setup();
    // The project's effective map is deliberately different from its own map, which is the override.
    store({ claude: "haiku-5-5", ghost: "global-ghost" });
    store({ codex: "gpt-5.2-codex-max", ghost: "own-ghost" }, PROJECT);
    window.location.hash = PROJECT_HASH;
    render(<SettingsScopeProvider section="threads"><TextGenerators /></SettingsScopeProvider>);

    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    expect(within(codexRow).getByText("Your choice")).toBeVisible();
    expect(within(codexRow).getByLabelText("Set for this project")).toBeVisible();
    expect(wireCalls("GET", `/api/text-generators?project=${PROJECT}`)).toHaveLength(1);
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex Max" }));
    await user.click(screen.getByRole("menuitem", { name: "GPT-5.2 Codex" }));
    await waitFor(() => expect(preferenceWrites()).toHaveLength(1));
    const [write] = preferenceWrites();
    expect(write?.scope).toEqual({ selection: { kind: "checkout", group_key: PROJECT_GROUP, project_id: PROJECT } });
    expect(write?.edits).toEqual({ [KEY]: { op: "set", value: { codex: "gpt-5.2-codex", ghost: "own-ghost" } } });
    expect(stored(PROJECT)).toEqual({ codex: "gpt-5.2-codex", ghost: "own-ghost" });
    expect(stored()).toEqual({ claude: "haiku-5-5", ghost: "global-ghost" });
  });

  it("returns a project's own engine to Automatic without touching its other own entries", async () => {
    const user = userEvent.setup();
    store({ claude: "haiku-5-5", ghost: "global-ghost" });
    store({ codex: "gpt-5.2-codex-max", ghost: "own-ghost" }, PROJECT);
    window.location.hash = PROJECT_HASH;
    render(<SettingsScopeProvider section="threads"><TextGenerators /></SettingsScopeProvider>);

    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex Max" }));
    await user.click(screen.getByRole("menuitem", { name: "Automatic" }));

    await waitFor(() => expect(preferenceWrites()).toHaveLength(1));
    expect(preferenceWrites()[0]?.scope).toEqual({ selection: { kind: "checkout", group_key: PROJECT_GROUP, project_id: PROJECT } });
    expect(preferenceWrites()[0]?.edits).toEqual({ [KEY]: { op: "set", value: { ghost: "own-ghost" } } });
    expect(stored(PROJECT)).toEqual({ ghost: "own-ghost" });
  });

  it("draws the layers icon only when the project's own map names the engine", async () => {
    store({ claude: "haiku-5-5" });
    store({ codex: "gpt-5.2-codex-max" }, PROJECT);
    window.location.hash = PROJECT_HASH;
    render(<SettingsScopeProvider section="threads"><TextGenerators /></SettingsScopeProvider>);

    await screen.findByText("Codex");
    expect(within(rowOf("Codex")).getByLabelText("Set for this project")).toBeVisible();
    expect(within(rowOf("Claude Code")).queryByLabelText("Set for this project")).toBeNull();
  });
});
