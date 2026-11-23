import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it } from "vitest";
import type { MockApi } from "../../../../mock/api";
import { installMockFetch } from "../../../test-support";
import { WorktreesSection } from "./WorktreesSection";
import { sentEdits } from "../rows/preferenceWrites.test-support";

let mock: MockApi;
beforeEach(() => {
  mock = installMockFetch();
  window.location.hash = "";
});

/** The service with this naming mode and prefix already saved. */
const serve = (mode: string, prefix = "orbyn") =>
  mock.handle("PUT", "/api/preferences", { values: { branch_naming_mode: mode, branch_name_prefix: prefix } });

it("says what the chosen naming mode does, and shows the instructions only for Custom", async () => {
  serve("static");
  const { unmount } = render(<WorktreesSection />);
  expect(await screen.findByText("The prefix plus a fixed name.")).toBeVisible();
  expect(screen.getByRole("radio", { name: "From the thread" })).toBeVisible();
  expect(screen.queryByRole("textbox", { name: "Branch name instructions" })).toBeNull();
  unmount();
  serve("semantic");
  const second = render(<WorktreesSection />);
  expect(await screen.findByText("Named from the thread automatically.")).toBeVisible();
  expect(screen.queryByRole("textbox", { name: "Branch name instructions" })).toBeNull();
  second.unmount();
  serve("custom");
  render(<WorktreesSection />);
  expect(await screen.findByText("Named using your branch name instructions.")).toBeVisible();
  expect(screen.getByRole("textbox", { name: "Branch name instructions" })).toBeVisible();
});

it("keeps the prefix without a slash, says the branch name it makes, and saves it trimmed", async () => {
  serve("static", "orbyn");
  const user = userEvent.setup();
  render(<WorktreesSection />);
  expect(await screen.findByText("Added in front of every branch Orbyn creates. A / is added after the prefix, so orbyn gives orbyn/fix-login.")).toBeVisible();
  const field = screen.getByRole("textbox", { name: "Branch prefix" });
  await waitFor(() => expect(field).toBeEnabled());
  fireEvent.change(field, { target: { value: "feat/" } });
  field.focus();
  await user.tab();
  await waitFor(() => expect(sentEdits()).toEqual([{ branch_name_prefix: { op: "set", value: "feat" } }]));
});
