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
