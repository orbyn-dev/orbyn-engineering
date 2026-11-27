import { expect, test } from "@playwright/test";
import { chmodSync, copyFileSync, existsSync, mkdtempSync, readdirSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { startProject, type Service } from "./service";

/**
 * I53, provider discovery on the real service: the service looks for engines on its own PATH — one fake engine binary
 * named `cline` and one named `cursor-agent` (their executable names on Windows) are found and shown with their own
 * program path (ACP engines carry no version), and discovery installs nothing; an instance's configured `program` is the
 * program that is used; and nothing is written to the PATH folder by discovery.
 */

type Engine = { id: string; name: string; installed: boolean; path: string | null; version: string | null; instance_of?: string };

let service: Service;
let forgeBin = "";

const suffix = process.platform === "win32" ? ".exe" : "";
const executable = join(dirname(process.env.ORBYN_BIN ?? resolve(import.meta.dirname, "../../../target/debug/orbynd")), `orbyn-fake-engine${suffix}`);

/**
 * A PATH entry that runs the protocol double: a link where the OS allows one, and a copy on a stock Windows box, which
 * refuses a file symlink without Developer Mode (EPERM, or EACCES when a policy takes the right away). A copy runs the
 * same, so discovery reads the same on either machine.
 */
function linkEngine(name: string): string {
  const target = join(forgeBin, `${name}${suffix}`);
  if (!existsSync(target)) {
    try {
      symlinkSync(executable, target);
    } catch (error) {
      const code = (error as NodeJS.ErrnoException).code;
      if (process.platform !== "win32" || (code !== "EPERM" && code !== "EACCES")) throw error;
      copyFileSync(executable, target);
    }
  }
  chmodSync(target, 0o755);
  return target;
}

/**
 * The program path the service reports for a PATH entry. Windows resolves a bare engine name to its `.exe` and spells
 * the extension as PATHEXT does (`cline.EXE`), while a path it was given already carries the extension and reads back
 * as given; elsewhere the entry is reported as placed.
 */
function expectEnginePath(actual: string | null | undefined, entry: string) {
  if (process.platform === "win32") expect(actual?.toLowerCase()).toBe(entry.toLowerCase());
  else expect(actual).toBe(entry);
}

test.beforeAll(async () => {
  // A PATH folder of the service host holding two engine names, both links to the protocol double.
  forgeBin = mkdtempSync(join(tmpdir(), "orbyn-provider-discovery-"));
  for (const name of ["cline", "cursor-agent"]) linkEngine(name);
  ({ service } = await startProject({ discoverKnownEngines: true, path: [forgeBin], fakeEngines: false }));
});

test.afterAll(async () => {
  await service?.stop();
});

test("a fake engine named on PATH and a cursor-agent binary are discovered with their own path", async () => {
  const engines = await service.api<Engine[]>("GET", "/api/engines");
  const cline = engines.find((engine) => engine.id === "cline");
  expect(cline).toMatchObject({ id: "cline", name: "Cline", installed: true });
  expectEnginePath(cline?.path, join(forgeBin, `cline${suffix}`));
  // The Cursor engine is reached through its `cursor-agent` program, and the PATH lookup found that name.
  const cursor = engines.find((engine) => engine.id === "cursor");
  expect(cursor).toMatchObject({ id: "cursor", name: "Cursor", installed: true });
  expectEnginePath(cursor?.path, join(forgeBin, `cursor-agent${suffix}`));
  // ACP engines report no version: their probe is a path and auth state (the ACP driver's probe), not a version command.
  expect([cline?.version ?? null, cursor?.version ?? null]).toEqual([null, null]);

  // Discovery installs nothing: the PATH folder holds only the two entries the spec made.
  expect(readdirSync(forgeBin).sort()).toEqual([`cline${suffix}`, `cursor-agent${suffix}`]);
});

test("an instance's program override is the program the service uses", async () => {
  const override = linkEngine("custom-agent");
  const instance = { id: "pathless", driver: "_local_acp", name: "Pathless override", program: override, args: [], enabled: true };
  await service.api("PUT", "/api/preferences", { values: { provider_instances: [instance] } });
  const engines = await service.api<Engine[]>("GET", "/api/engines");
  expect(engines.find((engine) => engine.id === "pathless")).toMatchObject({ id: "pathless", name: "Pathless override", installed: true, path: override, instance_of: "_local_acp" });
  // The override is the program the service uses for this instance; it is not a known engine name, and it lives in the same
  // PATH folder, so the assertion is about the configured program rather than about PATH discovery.
  expect(readdirSync(forgeBin).sort()).toEqual([`cline${suffix}`, `cursor-agent${suffix}`, `custom-agent${suffix}`]);
});
