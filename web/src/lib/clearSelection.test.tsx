import { render } from "@testing-library/react";
import { expect, it } from "vitest";
import { clearSelection, useClearSelectionOnLeave } from "./clearSelection";

function Page({ routeKey }: { routeKey: string }) {
  useClearSelectionOnLeave(routeKey);
  return <p data-testid="text">Some selectable text</p>;
}

const select = (node: Node) => {
  const range = document.createRange();
  range.selectNodeContents(node);
  const selection = window.getSelection();
  selection?.removeAllRanges();
  selection?.addRange(range);
  return selection;
};

it("drops a selection when the route changes, and keeps it while the route stays", () => {
  const view = render(<Page routeKey="thread:a" />);
  const selection = select(view.getByTestId("text"));
  view.rerender(<Page routeKey="thread:a" />);
  expect(selection?.toString()).toBe("Some selectable text");
  view.rerender(<Page routeKey="pulls" />);
  expect(window.getSelection()?.toString()).toBe("");
});

it("drops a selection when the component unmounts", () => {
  const view = render(<Page routeKey="thread:a" />);
  select(view.getByTestId("text"));
  view.unmount();
  expect(window.getSelection()?.toString()).toBe("");
  expect(window.getSelection()?.isCollapsed).toBe(true);
});

it("does nothing with no selection", () => {
  window.getSelection()?.removeAllRanges();
  expect(() => clearSelection()).not.toThrow();
});
