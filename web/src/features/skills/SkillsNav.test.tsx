import { render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { api } from "../../api/client";
import { SkillsNav } from "./SkillsNav";

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
