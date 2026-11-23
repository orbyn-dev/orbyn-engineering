import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { NumberWithUnit } from "./GeneralSection";

function field(value: unknown, save = vi.fn()) {
  render(<NumberWithUnit value={value} unit="minutes" label="Git fetch interval" save={save} disabled={false} />);
  return { save, input: screen.getByRole("spinbutton", { name: "Git fetch interval" }) };
}

it("keeps a below-min value from being stored, whether typed or pasted", async () => {
  const user = userEvent.setup();
  const { save, input } = field(5);
  expect(input).toHaveAttribute("min", "1");

  await user.click(input);
  await user.clear(input);
  await user.type(input, "-5");
  await user.tab();
  expect(input).toHaveValue(1);
  expect(save).toHaveBeenCalledWith(1);

  save.mockClear();
  await user.click(input);
  await user.clear(input);
  await user.paste("0");
  await user.tab();
  expect(input).toHaveValue(1);
  expect(save).toHaveBeenCalledWith(1);
});

it("puts the stored value back when the field is emptied", async () => {
  const user = userEvent.setup();
  const { save, input } = field(5);

  await user.click(input);
  await user.clear(input);
  expect(input).toHaveValue(null);
  await user.tab();

  expect(input).toHaveValue(5);
  expect(save).not.toHaveBeenCalled();
});

it("clamps a value above the max it is given", async () => {
  const user = userEvent.setup();
  const save = vi.fn();
  render(<NumberWithUnit value={20} unit="seconds" label="Quiet time" save={save} disabled={false} min={10} max={120} />);
  const input = screen.getByRole("spinbutton", { name: "Quiet time" });

  await user.click(input);
  await user.clear(input);
  await user.paste("600");
  await user.tab();
  expect(input).toHaveValue(120);
  expect(save).toHaveBeenCalledWith(120);
});
