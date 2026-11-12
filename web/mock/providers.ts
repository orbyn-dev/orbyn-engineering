/**
 * The grouped provider list as the service builds it (`GET /api/providers`): the labels come from the
 * same facts the crew router filters read, so a mock engine that cannot be gated reads Unmanaged.
 * Only what the mock engines can show is derived; the rest are the service's fixed rows.
 */
import type { EngineStatus } from "../src/api/types.ts";
import type { ProviderEntry } from "../src/generated/ProviderEntry.ts";
import type { ProviderList } from "../src/generated/ProviderList.ts";

/** Known ACP engines the service has only run against a test agent, with the program each starts. */
const UNVERIFIED: Record<string, { program: string; own?: string }> = {
  cursor: { program: "cursor-agent", own: "Starts over ACP. Not verified on a real install." },
  kimi: { program: "kimi" },
};
const NO_DETAIL = ["cursor", "fx"];

const planned = (id: string, name: string, note: string): ProviderEntry =>
  ({ id, name, group: "planned", labels: ["planned"], note, registered: false, openable: false });

export function providerList(engines: EngineStatus[]): ProviderList {
  const rows: ProviderEntry[] = engines.map((engine) => {
    const family = engine.instance_of ?? engine.id;
    const broken = engine.compatibility?.status === "broken";
    const unverified = engine.installed && !broken && engine.tier === "acp" ? UNVERIFIED[family] : undefined;
    const labels: ProviderEntry["labels"] = [];
    if (!engine.installed) labels.push("not_found");
    if (engine.installed && broken) labels.push("cant_start");
    if (engine.installed && (!engine.capabilities.managed || !engine.capabilities.approvals)) labels.push("unmanaged");
    if (engine.installed && engine.compatibility && !broken) labels.push("limited_support");
    if (unverified) labels.push("not_verified");
    const group: ProviderEntry["group"] = !engine.installed ? "not_found" : broken ? "cant_start" : unverified ? (unverified.own ? "found" : "more") : "in_use";
    return { id: engine.id, name: engine.name, group, labels, registered: true, openable: !NO_DETAIL.includes(family),
      note: unverified ? unverified.own ?? unverified.program : engine.compatibility?.message };
  });
  // The fixed rows ride with the full fixture set, so a scenario that keeps two engines shows only those.
  const full = engines.some((engine) => engine.id === "cursor");
  return {
    providers: [
      ...rows,
      ...(full ? [{ id: "antigravity", name: "Antigravity", group: "cant_start", labels: ["cant_start"], registered: false, openable: false,
        note: "Antigravity.app is installed, but it has no ACP server Orbyn can start." } satisfies ProviderEntry] : []),
      planned("pi", "Pi", "Pi speaks its own protocol, not ACP. Orbyn has no driver for it yet."),
      planned("muse", "Muse Code", "Muse Code speaks its own protocol, not ACP. Orbyn has no driver for it yet."),
      planned("zcode", "ZCode", "Uses a private protocol. Not built."),
    ],
  };
}
