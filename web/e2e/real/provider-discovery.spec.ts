import { expect, test } from "@playwright/test";
import { chmodSync, existsSync, mkdtempSync, readdirSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { startProject, type Service } from "./service";

/**
 * I53, provider discovery on the real service: the service looks for engines on its own PATH — one fake engine binary
 * named `cline` and one named `cursor-agent` are found and shown with their own program path (ACP engines carry no
 * version), and discovery installs nothing; an instance's configured `program` is the program that is used; and nothing
 * is written to the PATH folder by discovery.
 */

type Engine = { id: string; name: string; installed: boolean; path: string | null; version: string | null; instance_of?: string };

let service: Service;
let forgeBin = "";

const executable = join(dirname(process.env.ORBYN_BIN ?? resolve(import.meta.dirname, "../../../target/debug/orbynd")), "orbyn-fake-engine");

test.beforeAll(async () => {
  // A PATH folder of the service host holding two engine names, both links to the protocol double.
  forgeBin = mkdtempSync(join(tmpdir(), "orbyn-provider-discovery-"));
  for (const name of ["cline", "cursor-agent"]) {
    const target = join(forgeBin, name);
    if (!existsSync(target)) symlinkSync(executable, target);
    chmodSync(target, 0o755);
  }
  ({ service } = await startProject({ discoverKnownEngines: true, path: [forgeBin], fakeEngines: false }));
});

test.afterAll(async () => {
  await service?.stop();
});

test("a fake engine named on PATH and a cursor-agent binary are discovered with their own path", async () => {
  const engines = await service.api<Engine[]>("GET", "/api/engines");
  const cline = engines.find((engine) => engine.id === "cline");
  expect(cline).toMatchObject({ id: "cline", name: "Cline", installed: true, path: join(forgeBin, "cline") });
  // The Cursor engine is reached through its `cursor-agent` program, and the PATH lookup found that name.
  const cursor = engines.find((engine) => engine.id === "cursor");
  expect(cursor).toMatchObject({ id: "cursor", name: "Cursor", installed: true, path: join(forgeBin, "cursor-agent") });
  // ACP engines report no version: their probe is a path and auth state (the ACP driver's probe), not a version command.
  expect([cline?.version ?? null, cursor?.version ?? null]).toEqual([null, null]);

  // Discovery installs nothing: the PATH folder holds only the two links the spec made.
  expect(readdirSync(forgeBin).sort()).toEqual(["cline", "cursor-agent"]);
});

test("an instance's program override is used instead of PATH", async () => {
  const override = join(forgeBin, "custom-agent");
  if (!existsSync(override)) symlinkSync(executable, override);
  chmodSync(override, 0o755);
  const instance = { id: "pathless", driver: "_local_acp", name: "Pathless override", program: override, args: [], enabled: true };
  await service.api("PUT", "/api/preferences", { values: { provider_instances: [instance] } });
  const engines = await service.api<Engine[]>("GET", "/api/engines");
  expect(engines.find((engine) => engine.id === "pathless")).toMatchObject({ id: "pathless", name: "Pathless override", installed: true, path: override, instance_of: "_local_acp" });
  // The override is the program the service uses for this instance; it is not a known engine name, and it lives in the same
  // PATH folder, so the assertion is about the configured program rather than about PATH discovery.
  expect(readdirSync(forgeBin).sort()).toEqual(["cline", "cursor-agent", "custom-agent"]);
});
