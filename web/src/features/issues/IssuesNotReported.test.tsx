import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { createIssuesMock } from "../../../mock/issues";
import { IssuesNav } from "./IssuesNav";
import { IssueMark } from "./IssueMark";
import { IssuesPage } from "./IssuesPage";
import { issuesApi, setIssuesApi } from "./api";

afterEach(() => { setIssuesApi(null); vi.unstubAllGlobals(); window.location.hash = ""; });

const notFound = () => new Response(JSON.stringify({ code: "not_found", message: "Not found." }), { status: 404 });
const paths = (fetching: ReturnType<typeof vi.fn>) => fetching.mock.calls.map(([url]) => new URL(String(url), "http://x").pathname);

it("draws no entry, no marker, no list and no error while the service answers 404, and asks for nothing but the trackers", async () => {
  const fetching = vi.fn(async (_url: string) => notFound());
  vi.stubGlobal("fetch", fetching);
  window.location.hash = "#/issues";
  render(<><IssuesNav current={false} /><IssueMark threadId="thread-1" /><IssuesPage /></>);
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, 20)); });
  expect(screen.queryByRole("link", { name: "Issues" })).toBeNull();
  expect(screen.queryByRole("img")).toBeNull();
  expect(await screen.findByText("This service does not report issues.")).toBeVisible();
  expect(screen.queryByRole("button", { name: "Filters" })).toBeNull();
  expect(screen.queryByRole("alert")).toBeNull();
  expect(new Set(paths(fetching))).toEqual(new Set(["/api/issues/trackers"]));
});

it("reads every write as unavailable on the default client", async () => {
  vi.stubGlobal("fetch", vi.fn(async () => notFound()));
  await expect(issuesApi().link("t", { tracker: "github", scope: "s", key: "1" })).rejects.toThrow();
  await expect(issuesApi().startThread({ tracker: "github", scope: "s", key: "1" }, { message: "m", project_id: "p" })).rejects.toThrow();
});

it("shows the sidebar entry once the service reports trackers", async () => {
  setIssuesApi(createIssuesMock());
  render(<IssuesNav current={false} />);
  expect(await screen.findByRole("link", { name: "Issues" })).toHaveAttribute("href", "#/issues");
});
