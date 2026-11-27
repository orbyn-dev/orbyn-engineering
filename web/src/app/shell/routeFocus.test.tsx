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
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/threads/t-1", name: "thread", ready: true, covered: "#/threads/t-1" } },
  );
  // A person on the thread page, on the control they are about to open Settings from.
  const opener = control("Open settings");
  const tabbedTo = control("Another control");
  act(() => opener.focus());
  expect(document.activeElement).toBe(opener);

  // Settings is chosen while its first chunk is still arriving: the route is Settings, the screen is not
  // on the page, and the ready gate holds the focus where it is.
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/threads/t-1" });
  expect(document.activeElement).toBe(opener);

  // Tabbing during that wait moves the keyboard on, which is the state the opener must survive.
  act(() => tabbedTo.focus());
  expect(document.activeElement).toBe(tabbedTo);

  // The chunk arrives: the screen takes the focus, and what opened it is still the control it was.
  view.rerender({ key: "#/settings", name: "settings", ready: true, covered: "#/threads/t-1" });
  expect(document.activeElement?.textContent).toBe("Screen title");

  // Closing Settings returns to that control, never to the one the tab reached during the wait.
  view.rerender({ key: "#/threads/t-1", name: "thread", ready: true, covered: "#/threads/t-1" });
  expect(document.activeElement).toBe(opener);
});

it("returns to the Settings opener when a close lands before its screen arrived", () => {
  const main = screen("Screen title");
  const ref = { current: main };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/threads/t-1", name: "thread", ready: true, covered: "#/threads/t-1" } },
  );
  const opener = control("Open settings");
  const tabbedTo = control("Another control");
  act(() => opener.focus());

  // Settings is chosen from the control while its first chunk is still arriving.
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/threads/t-1" });
  // Tabbing during that wait moves the keyboard to a control the shell keeps on every page.
  act(() => tabbedTo.focus());
  expect(document.activeElement).toBe(tabbedTo);

  // Escape closes Settings before its screen arrived: the page it covered is the page showing again, and
  // the wait moved nothing, so the focus goes back to what opened Settings rather than staying on the tab.
  view.rerender({ key: "#/threads/t-1", name: "thread", ready: true, covered: "#/threads/t-1" });
  expect(document.activeElement).toBe(opener);
});

it("records the page Settings covers, not the last page focus completed on", () => {
  const main = screen("Screen title");
  const ref = { current: main };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: true, covered: "#/usage" } },
  );
  const opener = control("Open settings");
  act(() => opener.focus());

  // Skills is chosen and its chunk is still arriving: Skills is the page on the page, Usage is the last
  // page focus completed on, and Settings opens from a control that stays on screen throughout.
  view.rerender({ key: "#/skills", name: "skills", ready: false, covered: "#/skills" });
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/skills" });
  view.rerender({ key: "#/settings", name: "settings", ready: true, covered: "#/skills" });
  expect(document.activeElement?.textContent).toBe("Screen title");

  // Closing Settings returns to that control: it names the page underneath, which is the page showing.
  view.rerender({ key: "#/skills", name: "skills", ready: true, covered: "#/skills" });
  expect(document.activeElement).toBe(opener);
});

it("keeps the Settings opener when the close lands before either screen is ready", () => {
  const main = screen("Screen title");
  const ref = { current: main };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: true, covered: "#/usage" } },
  );
  // The control that opened Settings, on the page Settings covers.
  const opener = control("Open settings");
  const tabbedTo = control("Another control");
  act(() => opener.focus());

  // Skills is chosen and its chunk is still arriving; Settings opens over it while its own chunk is
  // arriving too, so neither screen is on the page.
  view.rerender({ key: "#/skills", name: "skills", ready: false, covered: "#/skills" });
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/skills" });
  // Tabbing during that wait moves the keyboard to a control the shell keeps on every page.
  act(() => tabbedTo.focus());
  expect(document.activeElement).toBe(tabbedTo);

  // Escape closes Settings before either screen arrived. The page it covered is still arriving, so the
  // focus cannot go back yet, but the opener it was opened from has to survive the wait.
  view.rerender({ key: "#/skills", name: "skills", ready: false, covered: "#/skills" });
  expect(document.activeElement).toBe(tabbedTo);

  // Skills arrives: the focus returns to what opened Settings on that page, not to the tab the wait
  // reached and not to the page title.
  view.rerender({ key: "#/skills", name: "skills", ready: true, covered: "#/skills" });
  expect(document.activeElement).toBe(opener);
});

it("drops the kept opener when the covered page is left before it arrives", () => {
  const main = screen("Screen title");
  const ref = { current: main };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: true, covered: "#/usage" } },
  );
  const opener = control("Open settings");
  const tabbedTo = control("Another control");
  act(() => opener.focus());

  // Settings opens over Skills while neither screen is ready, and closes back onto Skills the same way.
  view.rerender({ key: "#/skills", name: "skills", ready: false, covered: "#/skills" });
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/skills" });
  act(() => tabbedTo.focus());
  view.rerender({ key: "#/skills", name: "skills", ready: false, covered: "#/skills" });

  // Another page is chosen before Skills arrived: the opener belonged to Skills, so nothing may restore
  // it, and the page that does arrive takes the focus itself.
  view.rerender({ key: "#/pulls", name: "pulls", ready: false, covered: "#/pulls" });
  view.rerender({ key: "#/pulls", name: "pulls", ready: true, covered: "#/pulls" });
  expect(document.activeElement).not.toBe(opener);
  expect(document.activeElement?.textContent).toBe("Screen title");
});

it("spends the first focus handoff when it restores an opener on a cold load", () => {
  const main = screen("Screen title");
  // The shell's `#main` is focusable, so a skip past the title lands on the page itself.
  main.tabIndex = -1;
  const ref = { current: main };
  // A cold load: the first screen's chunk is still arriving, so nothing on it can take the focus yet.
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: false, covered: "#/usage" } },
  );
  const opener = control("Open settings");
  act(() => opener.focus());

  // Settings opens and closes before either screen paints, so its opener waits for the page it covered.
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/usage" });
  view.rerender({ key: "#/usage", name: "usage", ready: false, covered: "#/usage" });
  // Usage arrives: the opener is handed back, and that is the first handoff this load has made.
  view.rerender({ key: "#/usage", name: "usage", ready: true, covered: "#/usage" });
  expect(document.activeElement).toBe(opener);

  // That handoff is spent, so a destination chosen afterwards takes its own title like every later page;
  // it must not still be treated as the first, where the title is skipped and the page itself takes focus.
  view.rerender({ key: "#/pulls", name: "pulls", ready: true, covered: "#/pulls" });
  expect(document.activeElement).toBe(main.querySelector("[data-page-title]"));
});

it("hands focus to a page the route left and returned to while it was loading", () => {
  const usage = screen("Screen title");
  const ref = { current: usage };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: true, covered: "#/usage" } },
  );
  // The person moves to the navigation control they use to change pages.
  const nav = control("Skills");
  act(() => nav.focus());
  expect(document.activeElement).toBe(nav);

  // Skills is chosen while its chunk is still arriving, so nothing on it can take the focus yet.
  view.rerender({ key: "#/skills", name: "skills", ready: false, covered: "#/skills" });
  expect(document.activeElement).toBe(nav);

  // Back to Usage, whose screen mounts again and reports itself one commit after the route moved, as a
  // re-mounted screen does. The departure has to outlive that wait, so the page still follows the route
  // the second time instead of the return being read as the page that never changed.
  const remounted = screen("Screen title");
  usage.remove();
  ref.current = remounted;
  view.rerender({ key: "#/usage", name: "usage", ready: false, covered: "#/usage" });
  expect(document.activeElement).toBe(nav);
  view.rerender({ key: "#/usage", name: "usage", ready: true, covered: "#/usage" });
  expect(document.activeElement).toBe(remounted.querySelector("[data-page-title]"));
});

it("hands focus to a page the route left that shares its name", () => {
  const thread = screen("Screen title");
  const ref = { current: thread };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/threads/t-1", name: "thread", ready: true, covered: "#/threads/t-1" } },
  );
  const nav = control("Another thread");
  act(() => nav.focus());

  // Another thread is chosen before its data arrives: the page name is unchanged, the address is not.
  view.rerender({ key: "#/threads/t-2", name: "thread", ready: false, covered: "#/threads/t-2" });
  expect(document.activeElement).toBe(nav);

  // Back to the first thread: the name never changed, so only the address says the page moved.
  const remounted = screen("Screen title");
  thread.remove();
  ref.current = remounted;
  view.rerender({ key: "#/threads/t-1", name: "thread", ready: false, covered: "#/threads/t-1" });
  view.rerender({ key: "#/threads/t-1", name: "thread", ready: true, covered: "#/threads/t-1" });
  expect(document.activeElement).toBe(remounted.querySelector("[data-page-title]"));
});

it("keeps the focus where it was given when the same page only becomes ready", () => {
  const usage = screen("Screen title");
  const ref = { current: usage };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: true, covered: "#/usage" } },
  );
  // The phone's sheet closed over the same page: the route never changed, so nothing moved and the focus
  // stays where it was given (the title bar button, or the sheet's own Close).
  const given = control("Close");
  act(() => given.focus());
  view.rerender({ key: "#/usage", name: "usage", ready: false, covered: "#/usage" });
  view.rerender({ key: "#/usage", name: "usage", ready: true, covered: "#/usage" });
  expect(document.activeElement).toBe(given);
});

it("records a second opener after a session that ended before its screen arrived", () => {
  const main = screen("Screen title");
  const ref = { current: main };
  const view = renderHook(
    (props: { key: string; name: string; ready: boolean; covered: string }) =>
      useRouteFocus(props.key, props.name, props.ready, ref, props.covered),
    { initialProps: { key: "#/usage", name: "usage", ready: true, covered: "#/usage" } },
  );
  const first = control("Open settings");
  const second = control("Open settings again");

  // Settings is chosen from the first control while its chunk is arriving.
  act(() => first.focus());
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/usage" });
  // The person leaves Settings before that first chunk arrives, so the session never completed.
  view.rerender({ key: "#/usage", name: "usage", ready: false, covered: "#/usage" });

  // Opening it again from another control must record that control, not keep the abandoned one.
  act(() => second.focus());
  view.rerender({ key: "#/settings", name: "settings", ready: false, covered: "#/usage" });
  view.rerender({ key: "#/settings", name: "settings", ready: true, covered: "#/usage" });
  view.rerender({ key: "#/usage", name: "usage", ready: true, covered: "#/usage" });
  expect(document.activeElement).toBe(second);
});
