import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { PanelEmpty } from "./PanelEmpty";

it("says Not a Git repository and greys out Changes for a folder outside Git; the other choices work", () => {
  render(<PanelEmpty onOpen={() => undefined} gitless />);
  expect(screen.getByRole("button", { name: "Changes" })).toBeDisabled();
  expect(screen.getByRole("button", { name: "Browser" })).toBeEnabled();
  expect(screen.getByText("Not a Git repository.")).toBeInTheDocument();
});

it("offers Changes, and says nothing about Git, for a repository", () => {
  render(<PanelEmpty onOpen={() => undefined} />);
  expect(screen.getByRole("button", { name: "Changes" })).toBeEnabled();
  expect(screen.queryByText("Not a Git repository.")).toBeNull();
});
