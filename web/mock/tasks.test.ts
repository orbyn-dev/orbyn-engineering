import { expect, it } from "vitest";
import { createMockApi } from "./api";

it("serves the task list only at the collection, and each subpath its own shape", () => {
  const api = createMockApi();
  const list = api.handle("GET", "/api/tasks");
  expect(list.status).toBe(200);
  const tasks = list.body as { id: string; name: string; schedule: unknown }[];
  // The task the subpaths below are asked about is really in the list, so none of them is vacuous.
  const hook = tasks.find((task) => task.id === "task-hook");
  expect(hook).toBeDefined();

  // Copy on read: a caller holding this snapshot cannot change the mock's own tasks.
  const name = hook!.name;
  hook!.name = "changed by a caller";
  const reread = api.handle("GET", "/api/tasks").body as { id: string; name: string }[];
  expect(reread.find((task) => task.id === "task-hook")?.name).toBe(name);

  // A subpath must not fall back to the whole task array: the deliveries
  // dialog would render every task as a delivery. A delivery says what the
  // service did with it; a task says when it runs.
  const deliveries = api.handle("GET", "/api/tasks/task-hook/deliveries").body as Record<string, unknown>[];
  expect(deliveries.length).toBeGreaterThan(0);
  expect(deliveries.some((delivery) => delivery.id === "dlv-1")).toBe(true);
  expect(deliveries.every((delivery) => typeof delivery.status === "string" && !("schedule" in delivery))).toBe(true);

  // A task's runs answer for the task that was asked about, and only for a task that exists.
  const runs = api.handle("GET", "/api/tasks/task-hook/runs").body as { task_id: string; outcome: string }[];
  expect(runs.length).toBeGreaterThan(0);
  expect(runs.every((run) => run.task_id === "task-hook" && typeof run.outcome === "string")).toBe(true);
  expect(api.handle("GET", "/api/tasks/no-such-task/runs").status).toBe(404);

  // No task is served from its own path, and no unknown subpath either.
  expect(api.handle("GET", "/api/tasks/task-hook").status).toBe(404);
  expect(api.handle("GET", "/api/tasks/task-hook/nope").status).toBe(404);
});
