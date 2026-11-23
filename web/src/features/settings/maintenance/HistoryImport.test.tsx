import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import { installMockFetch } from "../../../test-support";
import { wire, wireWrites } from "../../../wire.test-support";
import { HistoryImport } from "./HistoryImport";

beforeEach(() => {
  installMockFetch();
});

// The mock service finds three Claude sessions: two in a trusted project, one in a folder Orbyn does not track.

it("scans only when asked and offers import only for trusted projects", async () => {
  const user = userEvent.setup();
  render(<HistoryImport />);
  expect(wire()).toEqual([]);
  await user.click(screen.getByRole("button", { name: "Find conversations" }));
  expect(await screen.findByText("Fix login redirect")).toBeInTheDocument();
  expect(screen.getAllByRole("button", { name: "Import" })).toHaveLength(2);
  expect(screen.getAllByRole("button", { name: "Add folder" })).toHaveLength(1);
  expect(screen.queryByRole("button", { name: "Open" })).toBeNull();
  await user.click(screen.getAllByRole("button", { name: "Import" })[0]!);
  await waitFor(() => expect(window.location.hash).toMatch(/^#\/threads\/t-\d+$/));
  expect(wireWrites()).toContainEqual({ method: "POST", path: "/api/history/import", body: { engine: "claude", session_id: "h1" } });
  // A session that already became a thread opens it rather than importing it again.
  await user.click(screen.getByRole("button", { name: "Scan again" }));
  expect(await screen.findByRole("button", { name: "Open" })).toBeInTheDocument();
  expect(screen.getAllByRole("button", { name: "Import" })).toHaveLength(1);
});

it("adds a session's folder as a project, which then needs trust", async () => {
  const user = userEvent.setup();
  render(<HistoryImport />);
  await user.click(screen.getByRole("button", { name: "Find conversations" }));
  await user.click(await screen.findByRole("button", { name: "Add folder" }));
  expect(await screen.findByRole("button", { name: "Trust folder" })).toBeVisible();
  expect(wireWrites()).toContainEqual({ method: "POST", path: "/api/projects/add", body: { path: "/Users/dev/code/vendor-sdk" } });
  expect(screen.queryByRole("button", { name: "Add folder" })).toBeNull();
  await user.click(screen.getByRole("button", { name: "Trust folder" }));
  expect(window.location.hash).toMatch(/^#\/trust\/p-\d+$/);
});
