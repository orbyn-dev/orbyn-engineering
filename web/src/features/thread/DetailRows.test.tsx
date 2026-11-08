import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { LimitValue, tokensShort } from "./DetailsCard";
import { DRow } from "./DetailRows";

it("shows each limit window as it is, amber at 20% left or less and still amber at 0%", () => {
  render(
    <LimitValue
      windows={[
        { label: "5h", used_percent: 38 },
        { label: "Weekly", used_percent: 82 },
        { label: "Monthly", used_percent: 100 },
      ]}
    />,
  );
  expect(screen.getByText("5h 62% left")).not.toHaveClass("amb");
  expect(screen.getByText("Weekly 18% left")).toHaveClass("amb");
  expect(screen.getByText("Monthly 0% left")).toHaveClass("amb");
});

it("keeps a loading row empty until its wait is over and then shows a still bar", () => {
  const view = render(<DRow icon={null} label="Local servers" loading waited={false} />);
  expect(view.container.querySelector(".sk")).toBeNull();
  view.rerender(<DRow icon={null} label="Local servers" loading waited skeleton={96} />);
  const bar = view.container.querySelector<HTMLElement>(".sk");
  expect(bar).not.toBeNull();
  expect(bar?.style.width).toBe("96px");
  expect(view.container.querySelector(".drow")).toHaveAttribute("aria-busy", "true");
});

it("shortens token counts as 38k, 1.5k and 1.5M", () => {
  expect([950, 1500, 38_000, 1_500_000, 2_000_000].map(tokensShort)).toEqual(["950", "1.5k", "38k", "1.5M", "2M"]);
});
