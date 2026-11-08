import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, ApiFailure } from "../../api/client";
import type { SkillPackStatus } from "../../generated/SkillPackStatus";
import { DRAFT_KEY, SCRATCH, useDraftStore } from "../../state/draftStore";
import { useUi } from "../../state/ui";
import { SkillsPage } from "./SkillsPage";

const catalog: SkillPackStatus = {
  version: "123", digest: "123", signature_valid: true, source: "/library",
  skills: [
    { name: "architect", category: "Engineering practice", description: "Shape contracts", enabled: true },
    { name: "design-review", category: "Product design", description: "Inspect interfaces", enabled: true },
    { name: "unknown", category: "Unrecognized", description: "A different specialty", enabled: false },
  ],
};

beforeEach(() => {
  vi.spyOn(api, "skills").mockResolvedValue(catalog);
  window.location.hash = "#/skills";
  useUi.getState().setDraft(DRAFT_KEY, "");
});
afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers(); });

it("renders names, descriptions and the library count from the API", async () => {
  render(<SkillsPage />);
  expect(await screen.findByRole("button", { name: /architect/ })).toHaveTextContent("Shape contracts");
  expect(screen.getByText("3 built in")).toBeVisible();
  expect(screen.getAllByText("Built in")).toHaveLength(3);
});

it("filters only known catalog categories and leaves unknown entries in All", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  await userEvent.click(screen.getByRole("button", { name: "Design" }));
  expect(screen.getByRole("button", { name: /design-review/ })).toBeVisible();
  expect(screen.queryByRole("button", { name: /architect/ })).toBeNull();
  expect(screen.queryByRole("button", { name: /unknown/ })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "All" }));
  expect(screen.getByRole("button", { name: /unknown/ })).toBeVisible();
  expect(screen.queryByRole("button", { name: "Project" })).toBeNull();
});

it("searches descriptions and names without regard to case", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  await userEvent.type(screen.getByRole("textbox", { name: "Search skills" }), "INTERFACES");
  expect(screen.getByRole("button", { name: /design-review/ })).toBeVisible();
  expect(screen.queryByRole("button", { name: /architect/ })).toBeNull();
});

it("opens a card with Space and shows only available detail fields", async () => {
  const user = userEvent.setup();
  const view = render(<SkillsPage />);
  const card = await screen.findByRole("button", { name: /architect/ });
  card.focus();
  await user.keyboard(" ");
  expect(window.location.hash).toBe("#/skills/architect");
  view.rerender(<SkillsPage skill="architect" />);
  expect(screen.getByRole("heading", { name: "architect" })).toHaveFocus();
  expect(screen.getByText("Shape contracts")).toBeVisible();
  expect(screen.queryByText("Used in")).toBeNull();
  expect(screen.queryByText("Files")).toBeNull();
  expect(screen.queryByText("Open folder")).toBeNull();
});

it("shows the approved message when a search has no matching skills", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  await userEvent.type(screen.getByRole("textbox", { name: "Search skills" }), "no-such-skill");
  expect(screen.getByRole("status")).toHaveTextContent("No skills match your search.");
  expect(screen.queryByRole("button", { name: /architect/ })).toBeNull();
});

it("preserves the draft when starting a new thread with an enabled skill", async () => {
  useUi.getState().setDraft(DRAFT_KEY, "Existing work");
  render(<SkillsPage skill="architect" />);
  await userEvent.click(await screen.findByRole("button", { name: "Use in a new thread" }));
  expect(useUi.getState().drafts[DRAFT_KEY]).toBe("Existing work $architect ");
  expect(window.location.hash).toBe("#/new");
});

it.each(["project-b", SCRATCH])("keeps the draft's %s destination and attachments", async (projectId) => {
  useUi.setState({ draftProject: projectId });
  useUi.getState().setDraft(DRAFT_KEY, "Existing work");
  const files = [new File(["draft context"], "notes.txt", { type: "text/plain" })];
  useDraftStore.getState().setFiles(files);
  render(<SkillsPage skill="architect" />);
  await userEvent.click(await screen.findByRole("button", { name: "Use in a new thread" }));
  expect(window.location.hash).toBe(`#/new/${projectId}`);
  expect(useUi.getState().drafts[DRAFT_KEY]).toBe("Existing work $architect ");
  expect(useDraftStore.getState().files).toEqual(files);
  useDraftStore.getState().setFiles([]);
});

it("does not offer the thread action for a skill outside the default selection", async () => {
  render(<SkillsPage skill="unknown" />);
  await screen.findByRole("heading", { name: "unknown" });
  expect(screen.queryByRole("button", { name: "Use in a new thread" })).toBeNull();
});

it("returns an unknown skill address to the library without displaying invented skill data", async () => {
  window.history.replaceState(null, "", "#/skills/missing-skill");
  const replace = vi.spyOn(window.history, "replaceState");
  const historyLength = window.history.length;
  render(<SkillsPage skill="missing-skill" />);
  await waitFor(() => expect(window.location.hash).toBe("#/skills"));
  expect(replace).toHaveBeenCalledWith(null, "", "#/skills");
  expect(window.history.length).toBe(historyLength);
  expect(screen.queryByText("missing-skill")).toBeNull();
});

it("shows a static skeleton only after 400 milliseconds", async () => {
  vi.useFakeTimers();
  vi.mocked(api.skills).mockReturnValue(new Promise(() => {}));
  const { container } = render(<SkillsPage />);
  expect(container).toBeEmptyDOMElement();
  await act(async () => { vi.advanceTimersByTime(399); });
  expect(container).toBeEmptyDOMElement();
  await act(async () => { vi.advanceTimersByTime(1); });
  expect(screen.getByRole("status", { name: "Loading skills" })).toBeVisible();
  expect(container.querySelectorAll(".skills-placeholder")).toHaveLength(6);
});

it("shows the failure and retries the catalog read", async () => {
  vi.mocked(api.skills).mockRejectedValueOnce(new ApiFailure("unavailable", "Catalog unavailable.", 503)).mockResolvedValueOnce(catalog);
  render(<SkillsPage />);
  expect(await screen.findByRole("alert")).toHaveTextContent("Catalog unavailable.");
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByRole("button", { name: /architect/ })).toBeVisible();
});
