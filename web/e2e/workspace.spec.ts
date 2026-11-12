import { expect, test, type Page } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto("/__mock/scenario/default");
});

for (const [width, height] of [
  [1440, 900],
  [1024, 768],
  [390, 844],
  [320, 740],
]) {
  test(`no page overflow at ${width}x${height}`, async ({ page }) => {
    await page.setViewportSize({ width: width!, height: height! });
    for (const hash of ["#/threads/t-done", "#/new", "#/settings/projects", "#/settings/providers"]) {
      await page.goto(`/${hash}`);
      await page.waitForTimeout(300);
      const overflow = await page.evaluate(() =>
        Math.max(
          ...[document.documentElement, ...document.querySelectorAll<HTMLElement>(".main, .transcript, .settings-main")].map(
            (element) => element.scrollWidth - element.clientWidth,
          ),
        ),
      );
      expect(overflow, hash).toBe(0);
    }
  });
}

test("token moves out of the URL", async ({ page }) => {
  await page.goto("/?token=abc#/");
  await expect(page.getByRole("navigation", { name: "Workspace" })).toBeVisible();
  expect(page.url()).not.toContain("token");
  expect(await page.evaluate(() => sessionStorage.getItem("orbyn.token"))).toBe("abc");
});

test("a dropped stream shows reconnecting and loses nothing", async ({ page }) => {
  await page.goto("/#/threads/t-empty");
  const box = page.getByRole("textbox", { name: /^Message/ });
  await box.fill("stream please");
  await box.press("Enter");
  await expect(page.getByText(/^Work(ed|ing) for/)).toBeVisible();
  await page.evaluate(() => fetch("/__mock/drop-streams"));
  // A stream that cannot reconnect is the service not answering: the header says Paused and the composer carries the banner.
  await expect(page.getByRole("status").filter({ hasText: "Can’t reach the Orbyn service" })).toBeVisible();
  await expect(page.locator(".thread-state")).toHaveText("Completed", { timeout: 10_000 });
  await expect(page.locator(".msg-assistant")).toContainText("Stop the run with Cancel, or wait for it to finish.");
});

test("the terminal connects and sizes itself without WebGL", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await page.getByRole("button", { name: "Terminal" }).click();
  await expect(page.locator(".terminal-panel .pill")).toHaveText("Connected");
  await expect(page.locator(".xterm-rows")).toContainText("resized to");
  expect(await page.locator(".xterm canvas").count()).toBe(0);
});

test("narrow layout is one surface with a way back", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await page.goto("/");
  await page.getByRole("link", { name: /Explain how the event log/ }).click();
  await expect(page.getByRole("navigation", { name: "Workspace" })).toBeHidden();
  await page.getByRole("button", { name: "Details" }).click();
  await page.getByRole("button", { name: /^Changes/ }).click();
  await expect(page.getByRole("tablist", { name: "Panel tabs" })).toBeVisible();
  await page.getByRole("button", { name: "Back to thread" }).click();
  await page.getByRole("link", { name: "Back to threads" }).click();
  await expect(page.getByRole("navigation", { name: "Workspace" })).toBeVisible();
});

test("Escape closes a dialog and returns focus to its opener", async ({ page }) => {
  await page.goto("/#/settings/providers");
  const open = page.getByRole("button", { name: /^Updates available/ });
  await open.click();
  const dialog = page.getByRole("dialog", { name: "Update everything" });
  await expect(dialog).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(dialog).toBeHidden();
  await expect(open).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByRole("region", { name: "Settings" })).toBeHidden();
});

test("keyboard focus is visible and reduced motion stops animation", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.goto("/#/threads/t-approval");
  await expect(page.locator(".thread-state")).toContainText("Approval");
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeFocused();
  await page.keyboard.press("Shift+Tab");
  const outline = await page.evaluate(() => getComputedStyle(document.activeElement as Element).outlineStyle);
  expect(outline).toBe("solid");
  const duration = await page.evaluate(() => getComputedStyle(document.querySelector(".spin") as Element).animationDuration);
  expect(Number.parseFloat(duration)).toBeLessThan(0.01);
});

test("first run opens on the draft flow with nothing to set up", async ({ page }) => {
  await page.goto("/__mock/scenario/first-run");
  // The welcome steps open first and are never a gate: one click leaves them for the draft flow.
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("heading", { name: "Add your first project" })).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Workspace" })).toBeVisible();
  await expect(page.getByText(/set up/i)).toHaveCount(0);
});

test("unavailable skills get a quiet line, never a blocking screen", async ({ page }) => {
  await page.goto("/__mock/scenario/no-skills");
  await page.getByRole("button", { name: "Skip for now" }).click();
  await expect(page.getByRole("heading", { name: "Add your first project" })).toBeVisible();
  await expect(page.getByRole("status").filter({ hasText: "Skills aren't available right now" })).toBeVisible();
  // The empty home no longer offers a project-less start; its draft is still the scratch route.
  await page.evaluate(() => (location.hash = "#/new/scratch"));
  await expect(page.getByRole("textbox", { name: /^Message/ })).toBeVisible();
});

test("the Skills page opens from the sidebar, filters by the library's categories and returns focus to the opened card", async ({ page }) => {
  await page.getByRole("link", { name: /^Skills/ }).click();
  await expect(page.getByRole("heading", { name: "Skills", exact: true })).toBeVisible();
  const filters = page.getByRole("group", { name: "Filter skills" });
  await expect(filters.getByRole("button").first()).toHaveText("All");
  await filters.getByRole("button", { name: "Product design" }).click();
  await expect(page.getByRole("button", { name: /orbyn-ui-design/ })).toBeVisible();
  await expect(page.getByRole("button", { name: /architect/ })).toHaveCount(0);
  // A search that finds nothing in the selected category says which, and Clear search keeps the category.
  await page.getByRole("textbox", { name: "Search skills" }).fill("migrations");
  await expect(page.getByRole("region", { name: "Skills" }).getByRole("status")).toContainText("No skills match “migrations” in Product design");
  await page.getByRole("button", { name: "Clear search" }).click();
  await expect(filters.getByRole("button", { name: "Product design" })).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByRole("button", { name: /orbyn-ui-design/ })).toBeVisible();
  await filters.getByRole("button", { name: "All" }).click();
  await page.getByRole("textbox", { name: "Search skills" }).fill("migrations");
  await expect(page.getByRole("region", { name: "Skills" }).getByRole("status")).toContainText("No skills match “migrations”");
  await page.getByRole("button", { name: "Clear search" }).click();
  await page.getByRole("button", { name: /architect/ }).click();
  await expect(page).toHaveURL(/#\/skills\/architect$/);
  await page.goBack();
  await expect(page.getByRole("button", { name: /architect/ })).toBeFocused();
});

const openHero = async (page: Page, animationMs: number) => {
  await page.addInitScript(
    (ms) => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelAnimationMs: ms })),
    animationMs,
  );
  await page.goto("/__mock/scenario/default?to=/%23/threads/t-empty");
  await expect(page.getByRole("heading", { name: /What should we build/ })).toBeVisible();
};

const send = async (page: Page, text: string) => {
  const box = page.getByRole("textbox", { name: /^Message/ });
  await box.fill(text);
  await box.press("Enter");
};

test("the composer travels with upstream's easing and holds the send until it lands", async ({ page }) => {
  await openHero(page, 240);
  const start = page.locator(".composer-box").boundingBox();
  const startBox = await start;
  let posted = false;
  page.on("request", (request) => {
    if (request.url().includes("/runs")) posted = true;
  });
  await send(page, "travel");
  const timing = await page.evaluate(() => {
    const animation = document.getAnimations().find((a) => a.id === "composer-dock");
    if (!animation) return null;
    animation.pause();
    const effect = animation.effect?.getComputedTiming();
    const keyframes = (animation.effect as KeyframeEffect).getKeyframes();
    return { duration: effect?.duration, easing: effect?.easing, from: keyframes[0]?.transform, properties: Object.keys(keyframes[0] ?? {}) };
  });
  expect(timing?.duration).toBe(240);
  expect(timing?.easing).toBe("cubic-bezier(0.4, 0, 0.2, 1)");
  expect(timing?.from).toContain("translate3d(0px,");
  expect(timing?.properties).not.toContain("width");
  await page.waitForTimeout(150);
  expect(posted).toBe(false);
  await page.evaluate(() => document.getAnimations().find((a) => a.id === "composer-dock")?.finish());
  await expect.poll(() => posted).toBe(true);
  const end = await page.locator(".composer-box").boundingBox();
  expect(end!.y).toBeGreaterThan(startBox!.y + 100);
});

test("the default zero duration and reduced motion skip the transition", async ({ page }) => {
  await openHero(page, 0);
  await send(page, "instant");
  expect(await page.evaluate(() => document.getAnimations().some((a) => a.id === "composer-dock"))).toBe(false);
  await page.emulateMedia({ reducedMotion: "reduce" });
  await page.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelAnimationMs: 240 })));
  await page.goto("/__mock/scenario/default?to=/%23/threads/t-empty");
  await expect(page.getByRole("heading", { name: /What should we build/ })).toBeVisible();
  await send(page, "still");
  expect(await page.evaluate(() => document.getAnimations().some((a) => a.id === "composer-dock"))).toBe(false);
  await expect(page.locator(".msg-user").getByText("still", { exact: true })).toBeVisible();
});

test("status dots are static and the spinner is the only loop, 900 ms linear", async ({ page }) => {
  await page.goto("/#/threads/t-approval");
  const dot = page.locator(".sidebar .status-dot").first();
  await expect(dot).toBeVisible();
  const read = (locator: typeof dot) =>
    locator.evaluate((element) => {
      const style = getComputedStyle(element);
      const after = getComputedStyle(element, "::after");
      return { name: style.animationName, after: after.animationName, duration: style.animationDuration, easing: style.animationTimingFunction };
    });
  expect(await read(dot)).toMatchObject({ name: "none", after: "none" });
  const spin = page.locator(".spin").first();
  await expect(spin).toBeAttached();
  expect(await read(spin)).toMatchObject({ name: "spin", duration: "0.9s", easing: "linear" });
});

const contextMotion = (page: Page) =>
  page.evaluate(() => {
    const t = document.getAnimations().find((a) => a instanceof CSSTransition && (a.effect as KeyframeEffect).target?.matches(".context"));
    const timing = t?.effect?.getComputedTiming();
    return t ? { property: (t as CSSTransition).transitionProperty, duration: timing?.duration, easing: timing?.easing } : null;
  });

test("the side panel does not move when the page loads with it open", async ({ page }) => {
  // The remembered state is open (the panel is no longer opened by the window's width).
  await page.setViewportSize({ width: 2000, height: 900 });
  await page.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelOpen: true })));
  await page.goto("/__mock/scenario/default?to=/%23/threads/t-done");
  await expect(page.getByRole("complementary", { name: "Thread context" })).toBeVisible();
  expect(await contextMotion(page)).toBeNull();
});

test("the side panel slides in over 300 ms ease-out and out over 210 ms ease-in-out", async ({ page }) => {
  await page.setViewportSize({ width: 1000, height: 800 });
  await page.addInitScript(() => localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ panelOpen: false })));
  await page.goto("/__mock/scenario/default?to=/%23/threads/t-done");
  const panel = page.getByRole("complementary", { name: "Thread context" });
  await expect(page.getByRole("heading", { name: "Explain how the event log resumes" })).toBeVisible();
  await expect(panel).toBeHidden();
  await page.getByRole("button", { name: "Show side panel" }).click();
  await expect(panel).toBeVisible();
  expect(await contextMotion(page)).toEqual({ property: "translate", duration: 300, easing: "cubic-bezier(0.2, 0.8, 0.2, 1)" });
  // Closing while the slide-in still runs reverses it, and the browser then shortens the reversed
  // transition (204 ms instead of 210 ms on a slow runner). Let the opening settle first.
  await page.evaluate(() => Promise.all(document.getAnimations()
    .filter((a) => a instanceof CSSTransition && (a.effect as KeyframeEffect).target?.matches(".context"))
    .map((a) => a.finished)));
  expect(await contextMotion(page)).toBeNull();
  await page.getByRole("button", { name: "Close side panel" }).click();
  expect(await contextMotion(page)).toEqual({ property: "translate", duration: 210, easing: "cubic-bezier(0.4, 0, 0.2, 1)" });
  await expect(panel).toBeHidden();
});

test("disclosures animate height over 200 ms", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  const panel = page.locator(".work .collapsible-panel").first();
  await page.locator(".work-summary[aria-expanded]").first().click();
  const transition = await panel.evaluate((element) => {
    const style = getComputedStyle(element);
    return { property: style.transitionProperty, duration: style.transitionDuration };
  });
  expect(transition).toEqual({ property: "height", duration: "0.2s" });
  await expect(page.locator(".work button.tcr").first()).toBeVisible();
});

test("settings float over the workspace and close back to it", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await expect(page.getByRole("heading", { name: "Explain how the event log resumes" })).toBeVisible();
  const opener = page.getByRole("link", { name: "Settings" });
  await opener.click();
  await expect(page.getByRole("region", { name: "Settings" })).toBeVisible();
  await page.getByRole("button", { name: "Close settings" }).click();
  await expect(page).toHaveURL(/threads\/t-done/);
  // Closing returns focus to what opened it.
  await expect(opener).toBeFocused();
  // Escape closes from anywhere on the page, not only from inside it.
  await opener.click();
  await expect(page.getByRole("region", { name: "Settings" })).toBeVisible();
  await page.locator(".settings-side").evaluate(() => (document.activeElement as HTMLElement | null)?.blur());
  await page.keyboard.press("Escape");
  await expect(page.getByRole("region", { name: "Settings" })).toHaveCount(0);
  await expect(opener).toBeFocused();
});

test("the sidebar rail resizes and remembers", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  const rail = page.getByRole("separator", { name: "Resize sidebar" });
  const box = await rail.boundingBox();
  await page.mouse.move(box!.x + 8, 300);
  await page.mouse.down();
  await page.mouse.move(300, 300);
  await page.mouse.up();
  await expect.poll(async () => (await page.locator(".sidebar").boundingBox())?.width).toBe(300);
  await page.reload();
  await expect.poll(async () => (await page.locator(".sidebar").boundingBox())?.width).toBe(300);
});

test("code blocks get a language header and highlighted tokens", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await expect(page.locator(".code-lang").first()).toHaveText("rust");
  await expect(page.locator(".code-card code span[style]").first()).toBeAttached();
});

test("the draft docks its composer at the bottom with the heading above it", async ({ page }) => {
  for (const width of [1280, 1728]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto("/#/new");
    const heading = await page.locator(".hero-title").boundingBox();
    const composer = await page.locator(".composer-box").boundingBox();
    expect((heading?.y ?? 0) + (heading?.height ?? 0)).toBeLessThan(composer?.y ?? 0);
    expect(900 - ((composer?.y ?? 0) + (composer?.height ?? 0))).toBeLessThan(80);
  }
});

test("usage opens from the sidebar as a page", async ({ page }) => {
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.goto("/#/threads/t-done");
  await page.getByRole("link", { name: "Usage" }).click();
  const usage = page.getByRole("region", { name: "Usage" });
  await expect(usage.getByRole("heading", { name: "Daily processed tokens" })).toBeVisible();
  await usage.getByRole("tab", { name: "Usage" }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(usage.getByRole("tab", { name: "Limits" })).toHaveAttribute("aria-selected", "true");
  // Claude Code reads Not reported and a signed-out Codex says so: no bar, no figure.
  await expect(usage.getByText("Subscription window")).toBeVisible();
  await expect(usage.getByText("Codex is signed out. Sign in with the CLI, then check again.")).toBeVisible();
  await expect(usage.getByRole("meter")).toHaveCount(0);
  await page.keyboard.press("t");
  await expect(page).toHaveURL(/#\/usage/);
});

test("the draft keeps its text and files through a project switch and adding a folder", async ({ page }) => {
  await page.goto("/#/new");
  const message = page.getByRole("textbox", { name: "Message" });
  await message.fill("Keep this");
  await page.getByLabel("Attach files").first().setInputFiles({
    name: "notes.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("notes"),
  });
  await page.locator(".hero-project").click();
  const search = page.getByRole("textbox", { name: "Search projects" });
  await search.fill("market");
  await search.press("Enter");
  await expect(page.locator(".hero-project")).toHaveText("marketing-site");
  await expect(message).toHaveValue("Keep this");

  await page.locator(".hero-project").click();
  await page.getByRole("option", { name: "Add project" }).click();
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByLabel("Folder path").fill("/Users/dev/code/fresh-folder");
  await palette.getByLabel("Folder path").press("Enter");
  await page.getByRole("button", { name: "Trust folder" }).click();
  await expect(page.locator(".hero-project")).toHaveText("fresh-folder");
  await expect(page.getByRole("textbox", { name: "Message" })).toHaveValue("Keep this");
  await expect(page.getByRole("list", { name: "Attachments" })).toContainText("notes.txt");
});

test("New thread in opens a draft in the chosen project", async ({ page }) => {
  await page.goto("/#/new");
  await expect(page.locator(".hero-project")).toBeVisible();
  await page.keyboard.press("ControlOrMeta+k");
  const palette = page.getByRole("dialog", { name: "Command palette" });
  await palette.getByRole("button", { name: "New thread in..." }).click();
  await palette.getByRole("button", { name: "marketing-site" }).click();
  await expect(page.locator(".hero-project")).toHaveText("marketing-site");
});

test("a project with no threads opens its empty screen, and New thread opens the composer for it", async ({ page }) => {
  // The mock's one project without threads is not trusted yet; trust it here.
  await page.route("**/api/projects", async (route) => {
    const response = await route.fetch();
    const projects = (await response.json()) as { id: string; trusted: boolean }[];
    await route.fulfill({ response, json: projects.map((project) => (project.id === "p-new" ? { ...project, trusted: true } : project)) });
  });
  await page.goto("/#/projects/p-new");
  await expect(page.getByRole("heading", { name: "No threads yet" })).toBeVisible();
  await expect(page.locator(".empty-project .logo")).toHaveAttribute("data-logo", "48");
  await page.getByRole("button", { name: "New thread" }).click();
  await expect(page).toHaveURL(/#\/new\/p-new$/);
  await expect(page.locator(".hero-project")).toHaveText("vendor-sdk-with-a-rather-long-repository-name-for-layout");
});

test("a project with threads goes straight to the composer, and the empty text never flashes", async ({ page }) => {
  // The empty screen must never appear, even for a frame, while the thread list is still loading (the sidebar has its own rows).
  await page.addInitScript(`window.__seen = []; new MutationObserver(() => { const h = document.querySelector(".empty-project h1"); if (h) window.__seen.push(h.textContent); }).observe(document, { childList: true, subtree: true });`);
  // The scenario page is already loaded, so a reload is what runs the script in a fresh document.
  await page.goto("/#/projects/p-site");
  await page.reload();
  await expect(page.getByRole("textbox", { name: "Message" })).toBeVisible();
  await expect(page).toHaveURL(/#\/new\/p-site$/);
  expect(await page.evaluate("window.__seen")).toEqual([]);
});

test("the pull requests page lists and filters", async ({ page }) => {
  await page.goto("/#/threads/t-done");
  await page.getByRole("link", { name: "Pull requests" }).click();
  await expect(page.getByRole("heading", { name: "Pull requests", exact: true })).toBeVisible();
  await expect(page.getByRole("button", { name: /Add approval timeouts/ })).toBeVisible();
  await page.getByLabel("Search pull requests").fill("label:nothing ");
  await expect(page.getByText("No pull requests match")).toBeVisible();
  await page.getByRole("button", { name: "Clear filters" }).click();
  await expect(page.getByRole("button", { name: /Add approval timeouts/ })).toBeVisible();

  // With a project chosen, the empty state names it.
  await page.getByRole("button", { name: "All projects" }).click();
  await page.getByRole("menuitemradio", { name: /^(?!All projects)/ }).first().click();
  await page.getByLabel("Search pull requests").fill("label:nothing ");
  await expect(page.getByText(/^No pull requests in .+ match these filters\.$/)).toBeVisible();
});

test("Reading folds the sidebar away on a wide window, but a narrow home is still the thread list", async ({ page }) => {
  const reading = () =>
    page.addInitScript(() => {
      localStorage.setItem("orbyn.prefs.migrated", "1");
      localStorage.setItem("orbyn.prefs.v1", JSON.stringify({ workspaceLayout: "reading" }));
    });
  await reading();
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto("/#/");
  // The mock scenario already loaded the app at this address, so the init script needs a fresh load.
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-workspace", "reading");
  const nav = page.getByRole("navigation", { name: "Workspace" });
  await expect(nav).toBeVisible();
  await expect(nav.getByRole("link", { name: "New thread", exact: true })).toBeVisible();

  await page.setViewportSize({ width: 1280, height: 760 });
  await page.goto("/#/threads/t-done");
  await expect(page.locator(".sidebar")).toHaveCount(0);
  const toggle = page.getByRole("button", { name: "Show sidebar" });
  await expect(toggle).toHaveAttribute("aria-expanded", "false");
  await toggle.click();
  await expect(page.locator(".sidebar")).toBeVisible();
  await page.locator(".sidebar a").first().focus();
  await page.getByRole("button", { name: "Hide sidebar" }).click();
  await expect(page.locator(".sidebar")).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Show sidebar" })).toBeFocused();
});

async function openPanel(page: import("@playwright/test").Page) {
  await page.setViewportSize({ width: 1500, height: 900 });
  await page.goto("/__mock/scenario/default?to=/%23/threads/t-done");
  await page.getByRole("button", { name: "Show side panel" }).click();
  // The panel has no tab yet, so its strip is the + alone.
  await expect(page.getByRole("button", { name: "Open a tab" })).toBeVisible();
  // The panel slides in; wait until its left edge stops moving.
  await expect
    .poll(() =>
      page.locator(".context").evaluate(
        (panel) =>
          new Promise<boolean>((resolve) => {
            const x = panel.getBoundingClientRect().x;
            requestAnimationFrame(() => requestAnimationFrame(() => resolve(panel.getBoundingClientRect().x === x)));
          }),
      ),
    )
    .toBe(true);
}
/** Sets the panel's width, past the 320px the resizer allows, to see the strip at the widths the designs draw. */
const resize = async (page: import("@playwright/test").Page, width: number) => {
  await page.locator(".context").evaluate((panel, w) => {
    (panel as HTMLElement).style.minWidth = "0";
    (panel as HTMLElement).style.width = `${w}px`;
  }, width);
  // The strip refits on the next resize observation; wait for the layout to stop changing.
  await expect.poll(async () => {
    const first = JSON.stringify(await slots(page));
    await page.waitForTimeout(150);
    return first === JSON.stringify(await slots(page));
  }).toBe(true);
};
/** The left edge and width of each visible slot, plus the "..." and "+" buttons, by name. */
const slots = (page: import("@playwright/test").Page) =>
  page.evaluate(() =>
    [...document.querySelectorAll<HTMLElement>(".ctx-strip .ctx-slot")].map((slot) => {
      const box = slot.getBoundingClientRect();
      // From the strip's own left edge, so the panel sliding in or being resized doesn't count as a move.
      const left = slot.closest(".ctx-strip")!.getBoundingClientRect().x;
      return { name: slot.getAttribute("aria-label"), x: Math.round(box.x - left), w: Math.round(box.width), h: Math.round(box.height) };
    }),
  );

/** The side panel starts with no tab: opens these from the + launcher, the way a person does. */
async function openTabs(page: import("@playwright/test").Page, names: string[]) {
  for (const name of names) {
    await page.getByRole("button", { name: "Open a tab" }).click();
    await page.getByRole("menuitem", { name }).click();
    await expect(page.getByRole("tab", { name, exact: true })).toHaveAttribute("aria-selected", "true");
  }
}
const FIVE = ["Changes", "Browser", "Activity", "Side chat", "Tools"];

test("the side panel starts with no tab, offers three choices, and the + opens a tab from the launcher", async ({ page }) => {
  await openPanel(page);
  await expect(page.locator(".ctx-tabs").getByRole("tab")).toHaveCount(0);
  await expect(page.getByText("Nothing open")).toBeVisible();
  await expect(page.getByRole("button", { name: "Tab menu" })).toHaveCount(0);
  await page.getByRole("button", { name: "Open a tab" }).click();
  await expect(page.getByRole("menuitem")).toHaveText([/^Changes/, /^Browser/, "Activity", "Side chat", "Tools"]);
  await page.getByRole("menuitem", { name: "Browser" }).click();
  await expect(page.getByRole("tab", { name: "Browser" })).toHaveAttribute("aria-selected", "true");
  await expect(page.locator(".ctx-title b")).toHaveText("Browser");
  // The empty panel's own choices open a tab too, and closing every tab brings the empty panel back.
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Maximise panel" }).waitFor();
  await page.keyboard.press("Escape");
  await openTabs(page, ["Tools"]);
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Close all tabs" }).click();
  await expect(page.getByText("Nothing open")).toBeVisible();
  await page.locator(".panel-empty").getByRole("button", { name: "Changes" }).click();
  await expect(page.getByRole("tab", { name: "Changes" })).toHaveAttribute("aria-selected", "true");
});

test("a tab dragged along the strip lands where the line shows, and Reset tab order puts the default back", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, ["Changes", "Browser", "Tools"]);
  const slot = (name: string) => page.getByRole("tab", { name, exact: true });
  const from = (await slot("Tools").boundingBox())!;
  const to = (await slot("Changes").boundingBox())!;
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x - 30, from.y + 2, { steps: 6 });
  await page.mouse.move(to.x + 4, to.y + 2, { steps: 6 });
  await expect(page.locator(".ctx-ins")).toBeVisible();
  await page.keyboard.press("Escape");
  await page.mouse.up();
  await expect(page.locator(".ctx-ins")).toHaveCount(0);
  const order = () => page.getByRole("tablist", { name: "Panel tabs" }).getByRole("tab").evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute("aria-label")));
  expect(await order()).toEqual(["Changes", "Browser", "Tools"]);
  await page.mouse.move(from.x + from.width / 2, from.y + from.height / 2);
  await page.mouse.down();
  await page.mouse.move(from.x - 30, from.y + 2, { steps: 6 });
  await page.mouse.move(to.x + 4, to.y + 2, { steps: 6 });
  await page.mouse.up();
  expect(await order()).toEqual(["Tools", "Changes", "Browser"]);
  await page.getByRole("button", { name: "Open a tab" }).click();
  await page.getByRole("menuitem", { name: "Reset tab order" }).click();
  expect(await order()).toEqual(["Changes", "Browser", "Tools"]);
});

test("the tab menu closes the others or all of them, announces it, and Close all returns to the empty panel", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, ["Changes", "Browser", "Tools"]);
  await page.getByRole("tab", { name: "Browser" }).click();
  await page.getByRole("button", { name: "Tab menu" }).click();
  await expect(page.getByRole("menuitem")).toHaveText(["Close tab", "Close other tabs", "Close all tabs", "Maximise panel"]);
  await page.getByRole("menuitem", { name: "Close other tabs" }).click();
  await expect(page.locator(".ctx-tabs").getByRole("tab")).toHaveCount(1);
  await expect(page.getByRole("tab", { name: "Browser" })).toBeFocused();
  await expect(page.locator(".context [role=status]").last()).toHaveText("Other tabs closed");
  // One tab left: the menu has Maximise panel only.
  await page.getByRole("button", { name: "Tab menu" }).click();
  await expect(page.getByRole("menuitem")).toHaveText(["Maximise panel"]);
  await page.keyboard.press("Escape");
  await openTabs(page, ["Tools"]);
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Close all tabs" }).click();
  await expect(page.getByText("Nothing open")).toBeVisible();
  await expect(page.getByRole("button", { name: "Open a tab" })).toBeFocused();
  await expect(page.locator(".context [role=status]").last()).toHaveText("All tabs closed. Nothing is open.");
});

test("each thread remembers its own tabs, and closing the panel does not forget them", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, ["Changes", "Tools"]);
  await page.getByRole("tab", { name: "Tools" }).click();
  await page.evaluate(() => { window.location.hash = "#/threads/t-failed"; });
  await expect(page.getByText("Nothing open")).toBeVisible();
  await openTabs(page, ["Activity"]);
  await page.evaluate(() => { window.location.hash = "#/threads/t-done"; });
  await expect(page.getByRole("tablist", { name: "Panel tabs" }).getByRole("tab")).toHaveCount(2);
  await expect(page.getByRole("tab", { name: "Tools" })).toHaveAttribute("aria-selected", "true");
  // Closing the panel with the × and opening it again brings the tabs back.
  await page.getByRole("button", { name: "Close side panel" }).click();
  await expect(page.getByRole("button", { name: "Open a tab" })).toHaveCount(0);
  await page.getByRole("button", { name: "Show side panel" }).click();
  await expect(page.getByRole("tab", { name: "Tools" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tab", { name: "Changes" })).toBeVisible();
});

test("the panel's tabs are icon-only 32px slots that never move when another tab opens, and the arrow keys walk them", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, FIVE);
  const row = page.getByRole("tablist", { name: "Panel tabs" });
  await expect(row.getByRole("tab")).toHaveText(["", "", "", "", ""]);
  const names = await row.getByRole("tab").evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute("aria-label")));
  expect(names).toEqual(["Changes", "Browser", "Activity", "Side chat", "Tools"]);
  await expect(page.getByRole("button", { name: "More tabs" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Open a tab" })).toBeVisible();
  // The name of the open tab is on the line under the strip.
  await expect(page.locator(".ctx-title b")).toHaveText("Tools");
  // One tab stop: the chosen tab. The arrows move the choice, and End and Home reach the ends.
  const last = page.getByRole("tab", { name: "Tools" });
  await expect(last).toHaveAttribute("aria-selected", "true");
  await last.focus();
  await page.keyboard.press("ArrowLeft");
  await expect(page.getByRole("tab", { name: "Side chat" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByRole("tab", { name: "Side chat" })).toBeFocused();
  await page.keyboard.press("Home");
  await expect(page.getByRole("tab", { name: "Changes" })).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("End");
  await expect(page.getByRole("tab", { name: "Tools" })).toHaveAttribute("aria-selected", "true");
});

for (const width of [380, 320]) {
  test(`at ${width}px every slot is 32px and stays put as each tab is opened`, async ({ page }) => {
    await openPanel(page);
    await openTabs(page, FIVE);
    await resize(page, width);
    const before = await slots(page);
    expect(before.length).toBeGreaterThan(2);
    for (const slot of before) expect([slot.w, slot.h]).toEqual([32, 32]);
    const tabs = before.filter((slot) => !["More tabs", "Open a tab"].includes(slot.name!));
    for (const tab of tabs) {
      await page.getByRole("tab", { name: tab.name! }).click();
      await expect(page.getByRole("tab", { name: tab.name! })).toHaveAttribute("aria-selected", "true");
      await expect(page.locator(".ctx-title b")).toHaveText(tab.name!);
      expect(await slots(page)).toEqual(before);
    }
  });
}

test("all five tabs fit at 320px, so there is no ... button", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, FIVE);
  await resize(page, 320);
  await expect(page.getByRole("tab", { name: "Tools" })).toHaveAttribute("aria-selected", "true");
  const names = await page.getByRole("tablist", { name: "Panel tabs" }).getByRole("tab").evaluateAll((tabs) => tabs.map((tab) => tab.getAttribute("aria-label")));
  expect(names).toEqual(["Changes", "Browser", "Activity", "Side chat", "Tools"]);
  await expect(page.getByRole("button", { name: "More tabs" })).toHaveCount(0);
  await expect(page.getByRole("button", { name: "Open a tab" })).toBeVisible();
  await expect(page.getByRole("button", { name: "Close side panel" })).toBeVisible();
});

test("a tab picked from the ... menu takes the last visible slot, and the open tab always shows", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, FIVE);
  await page.getByRole("tab", { name: "Changes" }).click();
  await resize(page, 240);
  const before = await slots(page);
  const visible = before.filter((slot) => !["More tabs", "Open a tab"].includes(slot.name!)).map((slot) => slot.name);
  expect(visible).toEqual(["Changes", "Browser"]);
  await page.getByRole("button", { name: "More tabs" }).click();
  const menu = page.getByRole("menu", { name: "More tabs" });
  await expect(menu.getByRole("menuitem")).toHaveText(["Activity", "Side chat", "Tools"]);
  await menu.getByRole("menuitem", { name: "Side chat" }).click();
  await expect(page.getByRole("tab", { name: "Side chat" })).toHaveAttribute("aria-selected", "true");
  const after = await slots(page);
  expect(after.map((slot) => slot.name)).toEqual(["Changes", "Side chat", "More tabs", "Open a tab"]);
  // Only the last visible slot changed hands; every slot sits where it did.
  expect(after.map((slot) => slot.x)).toEqual(before.map((slot) => slot.x));
  // Escape closes the menu and gives focus back to the "..." button.
  await page.getByRole("button", { name: "More tabs" }).click();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(page.getByRole("button", { name: "More tabs" })).toBeFocused();
});

test("the line under the strip names the open tab, and its menu closes the tab", async ({ page }) => {
  await openPanel(page);
  await openTabs(page, FIVE);
  await page.getByRole("tab", { name: "Browser" }).click();
  await expect(page.locator(".ctx-title b")).toHaveText("Browser");
  await page.getByRole("button", { name: "Tab menu" }).click();
  await page.getByRole("menuitem", { name: "Close tab" }).click();
  await expect(page.getByRole("tab", { name: "Browser" })).toHaveCount(0);
  await expect(page.getByRole("tab", { name: "Activity" })).toBeFocused();
});
