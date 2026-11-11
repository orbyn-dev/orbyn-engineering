import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { LimitsRow, LimitValue, tokensShort } from "./DetailsCard";

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

it("shortens token counts as 38k, 1.5k and 1.5M", () => {
  expect([950, 1500, 38_000, 1_500_000, 2_000_000].map(tokensShort)).toEqual(["950", "1.5k", "38k", "1.5M", "2M"]);
});

it("reads Not reported on the Limits row for OpenCode, Claude and a custom engine", () => {
  for (const engine of ["opencode", "claude", "my-own-engine"]) {
    const { unmount } = render(<LimitsRow threadId="t-1" engine={engine} />);
    expect(screen.getByText("Limits").closest(".drow")).toHaveTextContent("Not reported");
    unmount();
  }
});

it("leaves Codex to its own window: no Not reported while it has none", async () => {
  render(<LimitsRow threadId="t-1" engine="codex" />);
  await new Promise((resolve) => setTimeout(resolve, 50));
  expect(screen.queryByText("Not reported")).toBeNull();
});
