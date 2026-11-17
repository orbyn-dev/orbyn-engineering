import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it } from "vitest";
import { Segmented } from "./Segmented";

const options = [{ value: "a", label: "A" }, { value: "b", label: "B" }];

it("keeps one Tab stop on the chosen option, or the first when none matches", () => {
  const { rerender } = render(<Segmented label="Pick" value="b" options={options} onChange={() => undefined} />);
  expect(screen.getByRole("radio", { name: "B" })).toHaveAttribute("tabindex", "0");
  expect(screen.getByRole("radio", { name: "A" })).toHaveAttribute("tabindex", "-1");
  rerender(<Segmented label="Pick" value="missing" options={options} onChange={() => undefined} />);
  expect(screen.getByRole("radio", { name: "A" })).toHaveAttribute("tabindex", "0");
  expect(screen.getByRole("radio", { name: "B" })).toHaveAttribute("tabindex", "-1");
});

it("skips a disabled option with the arrow keys and does not choose it by click", async () => {
  const chosen: string[] = [];
  const three = [{ value: "a", label: "A" }, { value: "b", label: "B", disabled: true }, { value: "c", label: "C" }];
  render(<Segmented label="Pick" value="a" options={three} onChange={(value) => chosen.push(value)} />);
  expect(screen.getByRole("radio", { name: "B" })).toBeDisabled();
  screen.getByRole("radio", { name: "A" }).focus();
  await userEvent.keyboard("{ArrowRight}");
  expect(chosen).toEqual(["c"]);
});
