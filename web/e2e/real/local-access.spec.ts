import { expect, test } from "@playwright/test";
import { request as httpRequest } from "node:http";
import { startService, type Service } from "./service";

/**
 * L27, the local access checks: the page's own connection uses this launch's token, the launch token is
 * the only credential, a foreign `Host` or `Origin` is refused before any handler, and a `Person` route
 * needs the person key: a browser gets `desktop_required`, a caller holding only the service token gets
 * `person_required`, and this launch's key is accepted. The row's remote permission scopes and remote
 * access are out of scope and not exercised here.
 */
test.describe.configure({ mode: "serial" });

/** No shell owns this service and the development override is off, as for a browser. */
let browser: Service;
/** A shell owns this service, so the person key exists and is enforced. */
let desktop: Service;

test.beforeAll(async () => {
  browser = await startService({ enforcePersonKey: true });
  desktop = await startService({ enforcePersonKey: true, parentStdin: true });
});

test.afterAll(async () => {
  await Promise.all([browser?.stop(), desktop?.stop()]);
});

interface Answer {
  status: number;
  body: unknown;
}

/** One request over `node:http`, so the `Host` header is exactly the one given here. */
function call(service: Service, method: string, path: string, headers: Record<string, string>, body?: string): Promise<Answer> {
  const url = new URL(service.url);
  return new Promise((resolve, reject) => {
    const request = httpRequest({ host: url.hostname, port: url.port, path, method, headers }, (response) => {
      let text = "";
      response.setEncoding("utf8");
      response.on("data", (chunk: string) => (text += chunk));
      response.on("end", () => resolve({ status: response.statusCode ?? 0, body: text === "" ? null : JSON.parse(text) }));
    });
    request.on("error", reject);
    request.end(body);
  });
}

/** The launch token the page's URL carries, the one credential every `/api` call needs. */
function authorized(service: Service): Record<string, string> {
  const token = new URL(service.page).searchParams.get("token") ?? "";
  return { Authorization: `Bearer ${token}` };
}

/** A Person write that changes nothing for a command no binding names, so only the guard answers. */
const RESET = { method: "POST", path: "/api/keybindings/reset", body: JSON.stringify({ command: "no.such.command" }) };

test("the page's own connection works and the launch token is the only credential", async ({ page }) => {
  const workspace = page.waitForResponse(
    (response) => response.request().method() === "GET" && new URL(response.url()).pathname === "/api/workspace",
  );
  await page.goto(browser.page);
  expect((await workspace).status()).toBe(200);
  await expect(page.getByRole("navigation", { name: "Workspace" })).toBeVisible();
  expect(await call(browser, "GET", "/api/info", {})).toStrictEqual({
    status: 401,
    body: { code: "unauthorized", message: "Sign in to use this service." },
  });
});

test("a foreign Host header is refused", async () => {
  expect(await call(browser, "GET", "/api/info", { ...authorized(browser), Host: "evil.test" })).toStrictEqual({
    status: 403,
    body: { code: "host_not_allowed", message: "This host is not allowed." },
  });
});

test("a foreign Origin is refused", async () => {
  expect(await call(browser, "GET", "/api/info", { ...authorized(browser), Origin: "http://evil.test" })).toStrictEqual({
    status: 403,
    body: { code: "origin_not_allowed", message: "This origin is not allowed." },
  });
});

test("a browser gets the shell-only refusal for a Person route", async () => {
  expect(await call(browser, RESET.method, RESET.path, { ...authorized(browser), "Content-Type": "application/json" }, RESET.body)).toStrictEqual({
    status: 403,
    body: { code: "desktop_required", message: "This action is only available in the desktop app." },
  });
});

test("a Person route refuses a token-only request and takes this launch's person key", async () => {
  const headers = { ...authorized(desktop), "Content-Type": "application/json" };
  expect(await call(desktop, RESET.method, RESET.path, headers, RESET.body)).toStrictEqual({
    status: 403,
    body: { code: "person_required", message: "This action needs the person at the desktop app." },
  });
  const key = desktop.personKey;
  expect(key).toBeTruthy();
  const accepted = await call(desktop, RESET.method, RESET.path, { ...headers, "x-orbyn-person": key ?? "" }, RESET.body);
  expect(accepted.status).toBe(200);
  expect((accepted.body as { rules: unknown[] }).rules.length).toBeGreaterThan(0);
});
