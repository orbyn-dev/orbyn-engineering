import { render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import { act } from "@testing-library/react";
import { useWorkspace } from "../../state/workspace";
import { SkillsNav } from "./SkillsNav";
import { SkillsPage } from "./SkillsPage";
import { resetSkillsStore } from "./skillsStore";

beforeEach(() => resetSkillsStore());
afterEach(() => vi.restoreAllMocks());

it("links the Tools group to the catalog and counts every built-in skill", async () => {
  vi.spyOn(api, "skills").mockResolvedValue({
    version: "revision", digest: "digest", signature_valid: true, source: null,
    skills: [
      { name: "one", description: "One", category: "Engineering practice", enabled: true },
      { name: "two", description: "Two", category: "Product design", enabled: false },
    ],
  });
  render(<SkillsNav route={{ name: "skills", skill: "one" }} />);
  expect(screen.getByText("Tools")).toBeVisible();
  const link = screen.getByRole("link", { name: /Skills/ });
  expect(link).toHaveAttribute("href", "#/skills");
  expect(link).toHaveAttribute("aria-current", "page");
  expect(await screen.findByText("2")).toBeVisible();
  expect(screen.queryByText("Memory")).not.toBeInTheDocument();
});

it("keeps the Skills link without inventing a count while the catalog is unavailable", async () => {
  vi.spyOn(api, "skills").mockRejectedValue(new Error("unavailable"));
  render(<SkillsNav route={{ name: "home" }} />);
  const link = screen.getByRole("link", { name: "Skills" });
  expect(link).not.toHaveAttribute("aria-current");
  expect(link).not.toHaveTextContent("0");
});

it("shows no count when there is no library", async () => {
  vi.spyOn(api, "skills").mockResolvedValue({ version: "", digest: "", signature_valid: false, source: null, skills: [] });
  render(<SkillsNav route={{ name: "home" }} />);
  await screen.findByRole("link", { name: "Skills" });
  expect(screen.queryByText("0")).toBeNull();
});

it("reads the library once for the sidebar and the page together", async () => {
  const read = vi.spyOn(api, "skills").mockResolvedValue({
    version: "r", digest: "d", signature_valid: true, source: null,
    skills: [{ name: "one", description: "One", category: "Engineering practice", enabled: true }],
  });
  render(<><SkillsNav route={{ name: "skills" }} /><SkillsPage /></>);
  expect(await screen.findByRole("button", { name: /one/ })).toBeVisible();
  expect(read).toHaveBeenCalledTimes(1);
});

it("reads the library again after it is rebuilt, so the count follows", async () => {
  const entry = (name: string) => ({ name, description: name, category: "Engineering practice", enabled: true });
  const read = vi
    .spyOn(api, "skills")
    .mockResolvedValueOnce({ version: "r", digest: "d", signature_valid: true, source: null, skills: [entry("one")] })
    .mockResolvedValueOnce({ version: "r2", digest: "d2", signature_valid: true, source: null, skills: [entry("one"), entry("two")] });
  const managed = (revision: string) => ({ data: { configured: true, default_url: "", library: { revision }, targets: [] }, error: null, loading: false }) as never;
  useWorkspace.setState({ managed: managed("a") });
  render(<SkillsNav route={{ name: "home" }} />);
  expect(await screen.findByText("1")).toBeVisible();
  await act(async () => useWorkspace.setState({ managed: managed("b") }));
  expect(await screen.findByText("2")).toBeVisible();
  expect(read).toHaveBeenCalledTimes(2);
});
