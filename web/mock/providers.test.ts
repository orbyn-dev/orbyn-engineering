import { expect, it } from "vitest";
import { engines } from "./fixtures";
import { providerList } from "./providers";

it("shows the refused Grok row in the full fixture when no Grok engine exists", () => {
  const rows = providerList(engines.filter((engine) => engine.id !== "grok")).providers.filter((entry) => entry.id === "grok");
  expect(rows).toHaveLength(1);
  expect(rows[0]).toMatchObject({ group: "not_found", labels: ["not_found"], registered: false, openable: false });
  expect(rows[0]?.note).toContain("not xAI’s Grok CLI");
});

it("prefers a registered Grok engine over the refused fixture row", () => {
  const cursor = engines.find((engine) => engine.id === "cursor")!;
  const grok = { ...cursor, id: "grok", name: "Grok Build" };
  const rows = providerList([...engines.filter((engine) => engine.id !== "grok"), grok]).providers.filter((entry) => entry.id === "grok");
  expect(rows).toHaveLength(1);
  expect(rows[0]).toMatchObject({ id: "grok", name: "Grok Build", registered: true });
  expect(rows[0]?.note ?? "").not.toContain("not xAI’s Grok CLI");
});

const acp = (id: string, name: string, installed = true, managed = false) => {
  const cursor = engines.find((engine) => engine.id === "cursor")!;
  return { ...cursor, id, name, tier: "acp" as const, installed, path: installed ? `/usr/local/bin/${id}` : null, version: null,
    capabilities: { ...cursor.capabilities, managed, approvals: managed } };
};

it("writes the program alone as an ACP row's line, with Unmanaged and Not verified", () => {
  const row = providerList([acp("kiro", "Kiro")]).providers.find((entry) => entry.id === "kiro");
  expect(row).toMatchObject({ group: "more", labels: ["unmanaged", "not_verified"], note: "kiro-cli", openable: false });
});

it("leaves Kimi Code without Unmanaged, because it asks before it acts", () => {
  const row = providerList([acp("kimi", "Kimi Code", true, true)]).providers.find((entry) => entry.id === "kimi");
  expect(row).toMatchObject({ group: "more", labels: ["not_verified"], note: "kimi", openable: false });
});

it("gives a missing ACP engine only Not found, and no program line to open", () => {
  const row = providerList([acp("kiro", "Kiro", false)]).providers.find((entry) => entry.id === "kiro");
  expect(row).toMatchObject({ group: "not_found", labels: ["not_found"], openable: false });
  expect(row?.note).toBeUndefined();
});

it("opens a detail page only for the beta families", () => {
  const cursor = engines.find((engine) => engine.id === "cursor")!;
  const opencode = { ...cursor, id: "opencode", name: "OpenCode", installed: true };
  const rows = providerList([acp("kimi", "Kimi Code", true, true), opencode]).providers;
  expect(rows.find((entry) => entry.id === "kimi")?.openable).toBe(false);
  expect(rows.find((entry) => entry.id === "opencode")?.openable).toBe(true);
  expect(providerList(engines).providers.find((entry) => entry.id === "claude")?.openable).toBe(true);
  expect(providerList(engines).providers.find((entry) => entry.id === "codex")?.openable).toBe(true);
});
