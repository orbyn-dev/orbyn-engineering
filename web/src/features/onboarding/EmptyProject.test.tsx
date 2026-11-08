import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { Project, Thread } from "../../api/types";
import { parseRoute, routeHref } from "../../state/route";
import { useWorkspace } from "../../state/workspace";
import { EmptyProject } from "./EmptyProject";

vi.mock("./DraftView", () => ({ DraftView: ({ projectId }: { projectId: string | null }) => <div data-testid="composer">{projectId}</div> }));

const project = (extra: Partial<Project> = {}) => ({ id: "p1", name: "orbyn-engineering", trusted: true, path: "/work/p1", ...extra }) as Project;

const load = (projects: Project[], threads: Record<string, Thread> = {}) =>
  useWorkspace.setState({ projects: { data: projects, loading: false, error: null }, threads });

beforeEach(() => {
  window.location.hash = "#/projects/p1";
});
afterEach(cleanup);

it("shows the 48 px mark, the headline and one New thread button for a project with no threads", () => {
  load([project()]);
  const { container } = render(<EmptyProject projectId="p1" />);
  expect(screen.getByRole("heading", { name: "No threads yet" })).toBeVisible();
  expect(screen.getByText("Start one in orbyn-engineering.")).toBeVisible();
  expect(container.querySelector("svg.logo")).toHaveAttribute("data-logo", "48");
  expect(screen.getAllByRole("button")).toHaveLength(1);
});

it("opens the new-thread composer route for that project", async () => {
  load([project()]);
  render(<EmptyProject projectId="p1" />);
  await userEvent.setup().click(screen.getByRole("button", { name: "New thread" }));
  expect(parseRoute(window.location.hash)).toEqual({ name: "new", projectId: "p1" });
});

it("hides itself when the project has threads, or is not trusted yet, and shows the composer", () => {
  load([project()], { t1: { id: "t1", project_id: "p1" } as Thread });
  render(<EmptyProject projectId="p1" />);
  expect(screen.getByTestId("composer")).toHaveTextContent("p1");
  expect(screen.queryByText("No threads yet")).toBeNull();
  act(() => load([project({ trusted: false })]));
  expect(screen.getByTestId("composer")).toBeInTheDocument();
});

it("draws nothing until the project is known", () => {
  load([]);
  const { container } = render(<EmptyProject projectId="p1" />);
  expect(container).toBeEmptyDOMElement();
});

it("has its own address", () => {
  expect(routeHref({ name: "project", projectId: "a b" })).toBe("#/projects/a%20b");
  expect(parseRoute("#/projects/a%20b")).toEqual({ name: "project", projectId: "a b" });
  expect(parseRoute("#/projects")).toEqual({ name: "home" });
});
