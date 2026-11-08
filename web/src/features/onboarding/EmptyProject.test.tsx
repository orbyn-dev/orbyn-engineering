import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ApiFailure } from "../../api/client";
import type { Project } from "../../api/types";
import { parseRoute, routeHref } from "../../state/route";
import { useWorkspace } from "../../state/workspace";
import { EmptyProject } from "./EmptyProject";

const project = (extra: Partial<Project> = {}) => ({ id: "p1", name: "orbyn-engineering", trusted: true, path: "/work/p1", ...extra }) as Project;
const ready = <T,>(data: T) => ({ data, error: null, loading: false });

const load = (projects: Project[], ids: string[] | null) =>
  useWorkspace.setState({ projects: ready(projects), threadIds: ids === null ? {} : { p1: ready(ids) } });

beforeEach(() => {
  window.location.hash = "#/projects/p1";
});
afterEach(cleanup);

it("shows the 48 px mark, the headline and one New thread button for a project with no threads", () => {
  load([project()], []);
  const { container } = render(<EmptyProject projectId="p1" />);
  expect(screen.getByRole("heading", { name: "No threads yet" })).toBeVisible();
  expect(screen.getByText("Start one in orbyn-engineering.")).toBeVisible();
  expect(container.querySelector("svg.logo")).toHaveAttribute("data-logo", "48");
  expect(screen.getAllByRole("button")).toHaveLength(1);
});

it("opens the new-thread composer route for that project", async () => {
  load([project()], []);
  render(<EmptyProject projectId="p1" />);
  await userEvent.setup().click(screen.getByRole("button", { name: "New thread" }));
  expect(parseRoute(window.location.hash)).toEqual({ name: "new", projectId: "p1" });
});

it("decides nothing until the project's thread list is known, then passes a project with threads on to the composer", () => {
  load([project()], null);
  const { container } = render(<EmptyProject projectId="p1" />);
  expect(screen.queryByText("No threads yet")).toBeNull();
  expect(container.querySelector(".loading")).not.toBeNull();
  expect(window.location.hash).toBe("#/projects/p1");
  act(() => load([project()], ["t1"]));
  expect(screen.queryByText("No threads yet")).toBeNull();
  expect(parseRoute(window.location.hash)).toEqual({ name: "new", projectId: "p1" });
});

it("passes a project that is not trusted yet on to the composer", () => {
  load([project({ trusted: false })], []);
  render(<EmptyProject projectId="p1" />);
  expect(screen.queryByText("No threads yet")).toBeNull();
  expect(parseRoute(window.location.hash)).toEqual({ name: "new", projectId: "p1" });
});

it("waits while the projects load", () => {
  useWorkspace.setState({ projects: { data: null, error: null, loading: true }, threadIds: {} });
  const { container } = render(<EmptyProject projectId="p1" />);
  expect(container.querySelector(".loading")).not.toBeNull();
  expect(screen.queryByText("Project not found")).toBeNull();
});

it("says so, with a way home, for a project that is gone", () => {
  load([], null);
  render(<EmptyProject projectId="p1" />);
  expect(screen.getByRole("heading", { name: "Project not found" })).toBeVisible();
  expect(screen.getByRole("link", { name: "Go to threads" })).toHaveAttribute("href", "#/");
});

it("offers Retry when the projects could not be loaded", async () => {
  const refreshProjects = vi.fn(async () => undefined);
  useWorkspace.setState({ projects: { data: null, error: new ApiFailure("network", "Offline", 0), loading: false }, threadIds: {}, refreshProjects });
  render(<EmptyProject projectId="p1" />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  expect(refreshProjects).toHaveBeenCalled();
});

it("offers Retry when the thread list could not be loaded", async () => {
  const refreshThreads = vi.fn(async () => undefined);
  useWorkspace.setState({ projects: ready([project()]), threadIds: { p1: { data: null, error: new ApiFailure("network", "Offline", 0), loading: false } }, refreshThreads });
  render(<EmptyProject projectId="p1" />);
  await userEvent.setup().click(screen.getByRole("button", { name: "Retry" }));
  expect(refreshThreads).toHaveBeenCalledWith("p1");
});

it("has its own address", () => {
  expect(routeHref({ name: "project", projectId: "a b" })).toBe("#/projects/a%20b");
  expect(parseRoute("#/projects/a%20b")).toEqual({ name: "project", projectId: "a b" });
  expect(parseRoute("#/projects")).toEqual({ name: "home" });
});
