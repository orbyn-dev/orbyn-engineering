import { render, screen, within } from "@testing-library/react";
import { expect, it } from "vitest";
import { RunConfirm } from "./RunConfirm";
import { confirmFromFailure, type RunConfirmation } from "./model";

const show = (confirmation: RunConfirmation) =>
  render(<RunConfirm name="Deploy" confirmation={confirmation} onRun={() => undefined} onClose={() => undefined} />);

it("shows each set's name and its key names under it, never a value", () => {
  show({ grants: ["env_values"], envSets: [{ name: "Staging", keys: ["API_TOKEN", "API_BASE"] }, { name: "Prod", keys: [] }] });
  expect(screen.getByText("Staging")).toBeVisible();
  const keys = screen.getByRole("list", { name: "Keys in Staging" });
  expect(within(keys).getByText("API_TOKEN")).toBeVisible();
  expect(within(keys).getByText("API_BASE")).toBeVisible();
  expect(screen.getByText("Prod")).toBeVisible();
  expect(screen.queryByRole("list", { name: "Keys in Prod" })).toBeNull();
});

it("shows no value-shaped extra field the answer carried", () => {
  const read = confirmFromFailure({
    grants: ["env_values"],
    env_sets: [{ name: "Staging", keys: ["API_TOKEN"], values: ["hunter2"], value: "hunter2" }],
  })!;
  show(read);
  expect(screen.getByText("API_TOKEN")).toBeVisible();
  expect(screen.queryByText(/hunter2/)).toBeNull();
  expect(screen.queryByText("values")).toBeNull();
});

it("cuts a long set name and key name with the plain-text helper", () => {
  const name = "N".repeat(80);
  const key = "K".repeat(100);
  show({ grants: ["env_values"], envSets: [{ name, keys: [key] }] });
  expect(screen.getByText(`${"N".repeat(60)}…`)).toBeVisible();
  expect(screen.getByText(`${"K".repeat(80)}…`)).toBeVisible();
});
