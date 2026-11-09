import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api, ApiFailure } from "../../api/client";
import type { SkillPackStatus } from "../../generated/SkillPackStatus";
import { DRAFT_KEY, SCRATCH, useDraftStore } from "../../product/state/draftStore";
import { useUi } from "../../product/state/ui";
import { SkillsPage } from "./SkillsPage";
import { resetSkillsStore, useSkillsStore } from "./skillsStore";

const catalog: SkillPackStatus = {
  version: "123", digest: "123", signature_valid: true, source: "/library",
  skills: [
    { name: "architect", category: "Engineering practice", description: "Shape contracts", enabled: true },
    { name: "design-review", category: "Product design", description: "Inspect interfaces", enabled: true },
    { name: "unknown", category: "Unrecognized", description: "A different specialty", enabled: false },
  ],
};

beforeEach(() => {
  resetSkillsStore();
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

it("builds the category pills from the library's own categories, in its order, with All first", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  const pills = within(screen.getByRole("group", { name: "Filter skills" })).getAllByRole("button").map((button) => button.textContent);
  expect(pills).toEqual(["All", "Engineering practice", "Product design", "Unrecognized"]);
  await userEvent.click(screen.getByRole("button", { name: "Product design" }));
  expect(screen.getByRole("button", { name: /design-review/ })).toBeVisible();
  expect(screen.queryByRole("button", { name: /architect/ })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "All" }));
  expect(screen.getByRole("button", { name: /unknown/ })).toBeVisible();
});

it("hides the pills until the skills carry a category", async () => {
  vi.mocked(api.skills).mockResolvedValue({ ...catalog, skills: catalog.skills.map((entry) => ({ ...entry, category: "" })) });
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  expect(screen.queryByRole("button", { name: "All" })).toBeNull();
  expect(screen.getByRole("textbox", { name: "Search skills" })).toBeVisible();
});

it("shows each card's category beside its status", async () => {
  render(<SkillsPage />);
  const card = await screen.findByRole("button", { name: /architect/ });
  expect(card).toHaveTextContent("Built inEngineering practice");
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

it("says what was searched, keeps the filters, and Clear search brings the library back", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  const status = screen.getByRole("status");
  await userEvent.type(screen.getByRole("textbox", { name: "Search skills" }), "migrations");
  // The same element announces it: it was mounted before the text arrived.
  expect(screen.getByRole("status")).toBe(status);
  expect(status).toHaveTextContent("No skills match \u201cmigrations\u201d");
  expect(status).not.toHaveTextContent(" in ");
  expect(screen.getByText("0 of 3")).toBeVisible();
  expect(screen.getByRole("button", { name: "All" })).toBeVisible();
  expect(screen.queryByRole("button", { name: /architect/ })).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Clear search" }));
  expect(screen.getByRole("button", { name: /architect/ })).toBeVisible();
  expect(screen.getByRole("textbox", { name: "Search skills" })).toHaveValue("");
});

it("names the selected category when a search finds nothing in it, and Clear search keeps the category", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  await userEvent.click(screen.getByRole("button", { name: "Product design" }));
  await userEvent.type(screen.getByRole("textbox", { name: "Search skills" }), "migrations");
  expect(screen.getByRole("status")).toHaveTextContent("No skills match \u201cmigrations\u201d in Product design");
  await userEvent.click(screen.getByRole("button", { name: "Clear search" }));
  expect(screen.getByRole("textbox", { name: "Search skills" })).toHaveValue("");
  // The pill stays selected; the person changes it separately.
  expect(screen.getByRole("button", { name: "Product design" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByRole("button", { name: /design-review/ })).toBeVisible();
  expect(screen.queryByRole("button", { name: /architect/ })).toBeNull();
});

it("says No skills in {Category}, with no button, when the selected category has none and nothing is searched", async () => {
  render(<SkillsPage />);
  await screen.findByRole("button", { name: /architect/ });
  await userEvent.click(screen.getByRole("button", { name: "Product design" }));
  // The library is read again and that category is gone from it.
  vi.mocked(api.skills).mockResolvedValue({ ...catalog, skills: catalog.skills.filter((entry) => entry.category !== "Product design") });
  await act(async () => {
    await useSkillsStore.getState().load();
  });
  expect(screen.getByRole("status")).toHaveTextContent("No skills in Product design");
  expect(screen.queryByRole("button", { name: "Clear search" })).toBeNull();
});

it("marks the opened card for the shell's own route focus, which runs after the page's effects", async () => {
  const view = render(<SkillsPage />);
  await userEvent.click(await screen.findByRole("button", { name: /design-review/ }));
  view.rerender(<SkillsPage skill="design-review" />);
  await screen.findByRole("heading", { name: "design-review" });
  expect(screen.getByRole("heading", { name: "design-review" })).toHaveAttribute("data-page-title");
  view.rerender(<SkillsPage />);
  const card = await screen.findByRole("button", { name: /design-review/ });
  expect(card).toHaveAttribute("data-autofocus");
  expect(screen.getByRole("button", { name: /architect/ })).not.toHaveAttribute("data-autofocus");
});

it("keeps focus on the card when the library is read again after going back", async () => {
  const view = render(<SkillsPage />);
  await userEvent.click(await screen.findByRole("button", { name: /design-review/ }));
  view.rerender(<SkillsPage skill="design-review" />);
  await screen.findByRole("heading", { name: "design-review" });
  view.rerender(<SkillsPage />);
  await waitFor(() => expect(screen.getByRole("button", { name: /design-review/ })).toHaveFocus());
  // A slow workspace read changes the library's revision, and the library arrives again as a new object.
  await act(async () => {
    useSkillsStore.setState({ data: { ...catalog, skills: [...catalog.skills] } });
  });
  expect(screen.getByRole("button", { name: /design-review/ })).toHaveFocus();
});

it("shows one quiet line, and no count, filters or grid, when the library has no skills", async () => {
  vi.mocked(api.skills).mockResolvedValue({ ...catalog, skills: [] });
  const { container } = render(<SkillsPage />);
  expect(await screen.findByRole("heading", { name: "Skills" })).toBeVisible();
  expect(screen.getByText("No skills in this workspace yet.")).toBeVisible();
  expect(screen.queryByText(/built in/)).toBeNull();
  expect(screen.queryByText("No skills match your search.")).toBeNull();
  expect(screen.queryByRole("textbox", { name: "Search skills" })).toBeNull();
  expect(screen.queryByRole("group", { name: "Filter skills" })).toBeNull();
  expect(container.querySelector(".skills-grid")).toBeNull();
});

it("shows the whole description on a skill's own page, with its category", async () => {
  const long = "Design focused contract tests, failing-before regressions or verification of critical properties; use for requested TDD, model checking and verification drift, with practical boundaries and explicit evidence limits.";
  vi.mocked(api.skills).mockResolvedValue({ ...catalog, skills: [{ name: "tdd", category: "Engineering practice", description: long, enabled: true }] });
  render(<SkillsPage skill="tdd" />);
  expect(await screen.findByText(long)).toBeVisible();
  expect(screen.getByText("Built in \u00b7 Engineering practice")).toBeVisible();
});

it("goes back from a skill to focus the card that was opened", async () => {
  const view = render(<SkillsPage />);
  const card = await screen.findByRole("button", { name: /design-review/ });
  await userEvent.click(card);
  view.rerender(<SkillsPage skill="design-review" />);
  await screen.findByRole("heading", { name: "design-review" });
  view.rerender(<SkillsPage />);
  await waitFor(() => expect(screen.getByRole("button", { name: /design-review/ })).toHaveFocus());
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
