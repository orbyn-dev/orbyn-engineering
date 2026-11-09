import { render } from "@testing-library/react";
import { expect, it } from "vitest";
import { DRow } from "./DetailRows";

it("keeps a loading row empty until its wait is over and then shows a still bar", () => {
  const view = render(<DRow icon={null} label="Local servers" loading waited={false} />);
  expect(view.container.querySelector(".sk")).toBeNull();
  view.rerender(<DRow icon={null} label="Local servers" loading waited skeleton={96} />);
  const bar = view.container.querySelector<HTMLElement>(".sk");
  expect(bar).not.toBeNull();
  expect(bar?.style.width).toBe("96px");
  expect(view.container.querySelector(".drow")).toHaveAttribute("aria-busy", "true");
});
