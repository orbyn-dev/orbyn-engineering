/**
 * `useRouteFocus` (app/shell/focus.ts): focus follows the screen, and what opened Settings is recorded
 * as the route enters it. Settings' screen is a chunk of its own, so the route is Settings before the
 * screen is on the page; the wait in between is exactly when tabbing moves the keyboard. These tests
 * drive the hook's own ordering with real DOM focus, because that ordering is the behavior.
 */
import { act, renderHook } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { useRouteFocus } from "./focus";

const added: HTMLElement[] = [];
afterEach(() => {
  for (const element of added.splice(0)) element.remove();
});

/** The routed screen's page, whose title takes the focus when the screen is ready. */
function screen(title: string): HTMLElement {
  const root = document.createElement("main");
  const heading = document.createElement("h1");
  heading.setAttribute("data-page-title", "");
  heading.tabIndex = -1;
  heading.textContent = title;
  root.append(heading);
  document.body.append(root);
  added.push(root);
  return root;
}

/** A control on the page, as a keyboard reaches it. */
function control(label: string): HTMLButtonElement {
  const button = document.createElement("button");
  button.textContent = label;
  document.body.append(button);
  added.push(button);
  return button;
}

it("records the Settings opener as the route enters it, not once the screen arrives", () => {
  const main = screen("Screen title");
  const ref = { current: main };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean }) => useRouteFocus(props.key, props.name, props.ready, ref),
    { initialProps: { key: "#/threads/t-1", name: "thread", ready: true } },
  );
  // A person on the thread page, on the control they are about to open Settings from.
  const opener = control("Open settings");
  const tabbedTo = control("Another control");
  act(() => opener.focus());
  expect(document.activeElement).toBe(opener);

  // Settings is chosen while its first chunk is still arriving: the route is Settings, the screen is not
  // on the page, and the ready gate holds the focus where it is.
  view.rerender({ key: "#/settings", name: "settings", ready: false });
  expect(document.activeElement).toBe(opener);

  // Tabbing during that wait moves the keyboard on, which is the state the opener must survive.
  act(() => tabbedTo.focus());
  expect(document.activeElement).toBe(tabbedTo);

  // The chunk arrives: the screen takes the focus, and what opened it is still the control it was.
  view.rerender({ key: "#/settings", name: "settings", ready: true });
  expect(document.activeElement?.textContent).toBe("Screen title");

  // Closing Settings returns to that control, never to the one the tab reached during the wait.
  view.rerender({ key: "#/threads/t-1", name: "thread", ready: true });
  expect(document.activeElement).toBe(opener);
});
