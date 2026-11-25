import { expect, test } from "@playwright/test";
import { startProject, type Service } from "./service";

/**
 * R8 Server core and R10 Diagnostics/usage, through the real service routes: info, settings, an
 * engine refresh, the keybinding write, the storage-cleanup report/run/preview, the diagnostics
 * pages (processes, host, process history, telemetry, trace), the triage playbook, the updates
 * read, usage and its refresh, forge discovery and the text-generator read. R17's getSettings and
 * listTextGeneratorModels are the same two routes.
 */
test.describe.configure({ mode: "serial" });

let service: Service;
let projectId = "";

interface CleanupReport { dry_run: boolean; ran_at: string; entries: unknown[] }
interface Diagnostics { processes: unknown[]; database?: unknown }

const api = <T,>(method: string, path: string, body?: unknown) => service.api<T>(method, path, body);
const token = () => new URL(service.page).searchParams.get("token") ?? "";
async function rawStatus(method: string, path: string): Promise<number> {
  const response = await fetch(`${service.url}${path}`, { method, headers: { Authorization: `Bearer ${token()}` } });
  return response.status;
}

test.beforeAll(async () => {
  ({ service, projectId } = await startProject({ fakeEngines: true, forge: true }));
});
test.afterAll(() => service.stop());

test("the server core routes answer: info, settings, engine refresh, keybindings and storage cleanup", async () => {
  const info = await api<{ version: string; remote?: unknown }>("GET", "/api/info");
  expect(typeof info.version).toBe("string");
  expect(info.version).not.toBe("");
  expect(typeof (await api<unknown>("GET", "/api/settings"))).toBe("object");
  const engines = await api<unknown[]>("POST", "/api/engines/refresh");
  expect(Array.isArray(engines)).toBe(true);

  const bindings = await api<{ rules?: unknown[] }>("GET", "/api/keybindings");
  expect(Array.isArray(bindings.rules)).toBe(true);
  const rule = (bindings.rules ?? [])[0] as { key?: string; command?: string } | undefined;
  if (rule?.key && rule.command) {
    const saved = await api<{ rules?: unknown[] }>("POST", "/api/keybindings", { key: rule.key, command: rule.command, remove: true });
    expect(Array.isArray(saved.rules)).toBe(true);
  }

  // No run yet: the kept report is absent (null), and the preview is the dry run.
  const report = await api<CleanupReport | null>("GET", "/api/storage/cleanup");
  expect(report === null || Array.isArray(report.entries)).toBe(true);
  const preview = await api<{ groups: unknown[]; count: number; entries: unknown[] }>("GET", "/api/storage/cleanup/preview");
  expect(Array.isArray(preview.groups)).toBe(true);
  expect(typeof preview.count).toBe("number");
  expect(Array.isArray(preview.entries)).toBe(true);
  // dry_run=true is the safe run-now form: the report names what a real run would remove.
  const ran = await api<CleanupReport>("POST", "/api/storage/cleanup?dry_run=true");
  expect(ran.dry_run).toBe(true);
  expect(Array.isArray(ran.entries)).toBe(true);
});

test("the diagnostics, triage and update reads answer", async () => {
  const diagnostics = await api<Diagnostics>("GET", "/api/diagnostics");
  expect(Array.isArray(diagnostics.processes)).toBe(true);
  expect(typeof (await api<unknown>("GET", "/api/diagnostics/host"))).toBe("object");
  expect(typeof (await api<unknown>("GET", "/api/diagnostics/processes/history"))).toBe("object");
  expect(typeof (await api<unknown>("GET", "/api/diagnostics/telemetry"))).toBe("object");
  expect(typeof (await api<unknown>("GET", "/api/diagnostics/trace?since_minutes=60"))).toBe("object");
  expect(typeof (await api<unknown>("GET", "/api/triage/playbook"))).toBe("object");
  expect(await api<unknown>("GET", "/api/updates")).not.toBeUndefined();
  // A process that is not this service tree's child is a not_found, not a kill.
  expect(await rawStatus("POST", "/api/diagnostics/processes/999999/stop")).toBe(404);
});

test("usage, forge discovery and the text-generator read answer", async () => {
  expect(typeof (await api<unknown>("GET", "/api/usage"))).toBe("object");
  expect(typeof (await api<unknown>("POST", "/api/usage/refresh"))).toBe("object");
  expect(typeof (await api<unknown>("GET", "/api/limits"))).toBe("object");
  const forges = await api<{ id: string; signed_in: boolean }[]>("GET", "/api/forges");
  expect(forges.some((forge) => forge.id === "github" && forge.signed_in)).toBe(true);
  expect(typeof (await api<unknown>("GET", "/api/text-generators"))).toBe("object");
  expect(typeof (await api<unknown>("GET", `/api/text-generators?project=${encodeURIComponent(projectId)}`))).toBe("object");
});
