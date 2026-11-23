import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ApiFailure, NETWORK_ERROR, request } from "../../../api/client";
import type { ProjectScopeCatalog } from "../../../generated/ProjectScopeCatalog";
import type { TextGeneratorEngine } from "../../../generated/TextGeneratorEngine";
import type { TextGenerators as TextGeneratorsView } from "../../../generated/TextGenerators";
import { SettingsScopeProvider } from "../rows/SettingsScopeContext";
import { TextGenerators } from "./TextGenerators";

vi.mock("../../../api/client", async (original) => ({ ...(await original<object>()), request: vi.fn() }));

const model = (id: string, label: string, extra: Partial<TextGeneratorEngine["models"][number]> = {}) => ({
  id, label, description: null, is_default: false, custom: false, options: [], ...extra,
});

const haiku = model("haiku-5-5", "Haiku 5.5");
const mini = model("gpt-5.2-mini", "GPT-5.2 mini");
const codex = model("gpt-5.2-codex", "GPT-5.2 Codex");
const max = model("gpt-5.2-codex-max", "GPT-5.2 Codex Max", { is_default: true });
const tuned = model("team-tuned-mini", "team-tuned-mini", { custom: true });

const claude: TextGeneratorEngine = {
  engine: "claude", name: "Claude Code", models: [haiku],
  chosen: null, resolved: "haiku-5-5", reason: "legacy",
};
const small: TextGeneratorEngine = {
  engine: "codex", name: "Codex", models: [mini, codex, max, tuned],
  chosen: null, resolved: "gpt-5.2-mini", reason: "small",
};
const none: TextGeneratorEngine = {
  engine: "cursor", name: "Cursor", models: [mini],
  chosen: null, resolved: null, reason: null,
};
const chosen: TextGeneratorEngine = {
  engine: "codex", name: "Codex", models: [mini, codex, max, tuned],
  chosen: "gpt-5.2-codex", resolved: "gpt-5.2-codex", reason: "chosen",
};
const noModels: TextGeneratorEngine = {
  engine: "opencode", name: "OpenCode", models: [],
  chosen: null, resolved: null, reason: null,
};

const view = (engines: TextGeneratorEngine[]): TextGeneratorsView => ({ engines });
const requestMock = vi.mocked(request);
const rowOf = (title: string) => screen.getByText(title).closest(".setting-line") as HTMLElement;

beforeEach(() => {
  requestMock.mockReset();
  requestMock.mockResolvedValue(view([claude, small, none]) as never);
});

afterEach(() => vi.useRealTimers());

describe("Settings › Threads › Text generators", () => {
  it("draws one row per engine with the frame's strings and what Automatic resolves to", async () => {
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

  it("shows a saved choice as Your choice, ticks it, tags Default and Added in settings, and keeps Automatic disabled", async () => {
    const user = userEvent.setup();
    requestMock.mockResolvedValue(view([chosen, none]) as never);
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    expect(within(codexRow).getByText("Your choice")).toBeVisible();
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" }));

    const automatic = screen.getByRole("menuitem", { name: "Automatic" });
    expect(automatic).toHaveAttribute("aria-disabled", "true");
    expect(automatic).toBeDisabled();
    expect(automatic).toHaveAttribute("title", "Can’t be cleared yet");
    expect(screen.getByRole("menuitem", { name: "GPT-5.2 Codex" })).toHaveAttribute("aria-current", "true");
    expect(screen.getByRole("menuitem", { name: "GPT-5.2 Codex Max" })).toHaveTextContent("Default");
    expect(screen.getByRole("menuitem", { name: "team-tuned-mini" })).toHaveTextContent("Added in settings");
  });

  it("sends exactly one ordinary preferences write for a chosen model", async () => {
    const user = userEvent.setup();
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    await user.click(within(codexRow).getByRole("button", { name: "Automatic" }));
    await user.click(screen.getByRole("menuitem", { name: "GPT-5.2 Codex" }));

    await waitFor(() => expect(requestMock.mock.calls.filter(([method]) => method === "PUT")).toHaveLength(1));
    expect(requestMock).toHaveBeenCalledWith("PUT", "/api/preferences", {
      values: { text_generation_models: { codex: "gpt-5.2-codex" } },
    });
  });

  it("shows the service's refusal under that row, keeps its saved choice, and reloads the list", async () => {
    const user = userEvent.setup();
    let reads = 0;
    requestMock.mockImplementation(async (method, path) => {
      if (method === "GET" && path.startsWith("/api/text-generators")) { reads += 1; return view([chosen, none]) as never; }
      if (method === "PUT" && path === "/api/preferences") {
        throw new ApiFailure("invalid_input", "Codex does not list gpt-5.2-codex-max as a model.", 400);
      }
      throw new Error(`unexpected ${method} ${path}`);
    });
    render(<TextGenerators />);
    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" }));
    await user.click(screen.getByRole("menuitem", { name: "GPT-5.2 Codex Max" }));

    expect(await within(codexRow).findByRole("alert")).toHaveTextContent("Codex does not list gpt-5.2-codex-max as a model.");
    expect(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex" })).toBeVisible();
    expect(within(codexRow).queryByRole("button", { name: "GPT-5.2 Codex Max" })).toBeNull();
    await waitFor(() => expect(reads).toBe(2));
  });

  it("waits 400 ms before drawing skeleton rows", async () => {
    vi.useFakeTimers();
    requestMock.mockImplementation(() => new Promise(() => undefined));
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
    let failed = true;
    requestMock.mockImplementation(async (method, path) => {
      if (method === "GET" && path.startsWith("/api/text-generators")) {
        if (failed) throw new ApiFailure(NETWORK_ERROR, "Can't reach the Orbyn service.", 0);
        return view([claude]) as never;
      }
      throw new Error(`unexpected ${method} ${path}`);
    });
    render(<TextGenerators />);
    expect(await screen.findByText("Couldn’t load text generators")).toBeVisible();
    expect(screen.getByText("Check that the Orbyn service is running.")).toBeVisible();
    failed = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Claude Code")).toBeVisible();
  });

  it("draws the empty line and opens Providers", async () => {
    const user = userEvent.setup();
    requestMock.mockResolvedValue(view([]) as never);
    window.location.hash = "#/settings/threads";
    render(<TextGenerators />);
    expect(await screen.findByText("No engine is on for this project")).toBeVisible();
    expect(screen.getByText("Titles and commit text need one. Turn an engine on in Providers.")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Open Providers" }));
    expect(window.location.hash).toBe("#/settings/providers");
  });

  it("turns the control off for an engine that listed no models", async () => {
    requestMock.mockResolvedValue(view([noModels]) as never);
    render(<TextGenerators />);
    await screen.findByText("OpenCode");
    const row = rowOf("OpenCode");
    expect(within(row).getByText("The engine returned no model list · the thread’s own model writes")).toBeVisible();
    expect(within(row).getByRole("button", { name: "No models listed" })).toBeDisabled();
  });

  it("reads and writes the project scope when the settings scope is a checkout", async () => {
    const user = userEvent.setup();
    const catalog: ProjectScopeCatalog = {
      version: "catalog", metadata_complete: true,
      selection: { kind: "checkout", group_key: "g1", project_id: "p1" },
      groups: [{ key: "g1", label: "Project one", members: [{
        project_id: "p1", physical_key: "p1", name: "Project one", path: "/project-one",
        repository_key: "p1", repository_label: "Project one", repository_relative_path: "",
        grouping_mode: "separate", metadata_complete: true, unavailable: null,
      }] }],
    };
    const projectEngine: TextGeneratorEngine = {
      ...chosen,
      project: { project_id: "p1", chosen: "gpt-5.2-codex-max", own: true, resolved: "gpt-5.2-codex-max", reason: "chosen" },
    };
    requestMock.mockImplementation(async (method, path) => {
      if (method === "POST" && path === "/api/settings/project-scopes") return catalog as never;
      if (method === "GET" && path.startsWith("/api/text-generators")) return view([projectEngine, claude]) as never;
      if (method === "PUT" && path === "/api/projects/p1/preferences") return [] as never;
      throw new Error(`unexpected ${method} ${path}`);
    });
    window.location.hash = "#/settings/threads?project=g1&checkout=p1";
    render(<SettingsScopeProvider section="threads"><TextGenerators /></SettingsScopeProvider>);

    await screen.findByText("Codex");
    const codexRow = rowOf("Codex");
    expect(within(codexRow).getByText("Your choice")).toBeVisible();
    expect(within(codexRow).getByLabelText("Set for this project")).toBeVisible();
    expect(requestMock).toHaveBeenCalledWith("GET", "/api/text-generators?project=p1", undefined, expect.any(AbortSignal));
    await user.click(within(codexRow).getByRole("button", { name: "GPT-5.2 Codex Max" }));
    await user.click(screen.getByRole("menuitem", { name: "GPT-5.2 Codex" }));
    await waitFor(() => expect(requestMock).toHaveBeenCalledWith("PUT", "/api/projects/p1/preferences", {
      values: { text_generation_models: { codex: "gpt-5.2-codex" } },
    }));
  });
});
