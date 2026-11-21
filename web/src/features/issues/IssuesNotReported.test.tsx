import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import { IssuesNav } from "./IssuesNav";
import { IssueMark } from "./IssueMark";
import { IssuesPage } from "./IssuesPage";
import { issuesApi, setIssuesApi } from "./api";

afterEach(() => { setIssuesApi(null); vi.unstubAllGlobals(); window.location.hash = ""; });

it("makes no request and draws no entry, no marker and no list while the service reports no issues", async () => {
  const fetching = vi.fn();
  vi.stubGlobal("fetch", fetching);
  window.location.hash = "#/issues";
  render(<><IssuesNav current={false} /><IssueMark threadId="thread-1" /><IssuesPage /></>);
  await act(async () => { await Promise.resolve(); });
  expect(screen.queryByRole("link", { name: "Issues" })).toBeNull();
  expect(screen.queryByRole("img")).toBeNull();
  expect(await screen.findByText("This service does not report issues.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Filters" })).toBeNull();
  expect(fetching).not.toHaveBeenCalled();
});

it("reads every write as unavailable on the default client", async () => {
  await expect(issuesApi().link("t", { tracker: "github", scope: "s", key: "1" })).rejects.toThrow();
  await expect(issuesApi().startThread({ tracker: "github", scope: "s", key: "1" }, { message: "m", project_id: "p" })).rejects.toThrow();
});

it("shows the sidebar entry once the service reports trackers", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesNav current={false} />);
  expect(await screen.findByRole("link", { name: "Issues" })).toHaveAttribute("href", "#/issues");
});
