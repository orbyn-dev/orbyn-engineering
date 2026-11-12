/**
 * The grouped provider list as the service builds it (`GET /api/providers`): the labels come from the
 * same facts the crew router filters read, so a mock engine that cannot be gated reads Unmanaged.
 * Only what the mock engines can show is derived; the rest are the service's fixed rows.
 */
import type { EngineStatus } from "../src/api/types.ts";
import type { ProviderEntry } from "../src/generated/ProviderEntry.ts";
import type { ProviderList } from "../src/generated/ProviderList.ts";

const planned = (id: string, name: string, note: string): ProviderEntry =>
  ({ id, name, group: "planned", labels: ["planned"], note, registered: false });

export function providerList(engines: EngineStatus[]): ProviderList {
  const found: ProviderEntry[] = engines.filter((engine) => engine.installed).map((engine) => {
    const broken = engine.compatibility?.status === "broken";
    const labels: ProviderEntry["labels"] = [];
    if (broken) labels.push("cant_start");
    if (!engine.capabilities.managed || !engine.capabilities.approvals) labels.push("unmanaged");
    if (engine.compatibility && !broken) labels.push("limited_support");
    return { id: engine.id, name: engine.name, group: broken ? "cant_start" : "in_use", labels, note: engine.compatibility?.message, registered: true };
  });
  return {
    providers: [
      ...found,
      { id: "antigravity", name: "Antigravity", group: "cant_start", labels: ["cant_start"], registered: false,
        note: "Antigravity.app is installed, but it has no ACP server Orbyn can start." },
      planned("pi", "Pi", "Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet."),
      planned("muse", "Muse Code", "Muse Code speaks its own protocol, not ACP. Orbyn has no driver for it yet."),
      planned("zcode", "ZCode", "Uses a private protocol. Not built."),
    ],
  };
}
