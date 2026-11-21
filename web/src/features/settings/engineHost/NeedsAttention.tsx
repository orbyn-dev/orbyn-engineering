import type { EngineStatus } from "../../../api/types";
import { Button } from "../../../ui/controls";
import { engineGap, plain, programOf, useEngineHosts, type Gap } from "../../enginehost";
import { SettingRow } from "../rows/SettingRow";
import { HostPill } from "./HostParts";
import "./engineHost.css";

/** An engine's gap, once the service reports engine hosts. Not found is left to the engine's own row, which already says it. */
function useGap(engine: EngineStatus): Gap | null {
  const { hosts } = useEngineHosts();
  if (!hosts) return null;
  const gap = engineGap(engine, hosts.find((row) => row.id === engine.id) ?? null);
  return gap && gap.kind !== "not_found" ? gap : null;
}

/**
 * The Needs attention group of the Providers page: the engines that can't run yet, and why. Each row says one thing and offers the
 * one action that can change it. Nothing is drawn until the service reports engine hosts.
 */
export function NeedsAttentionGroup({ engines, onOpen }: { engines: EngineStatus[]; onOpen(id: string): void }) {
  const { hosts } = useEngineHosts();
  if (!hosts) return null;
  const rows = engines.flatMap((engine) => {
    const gap = engineGap(engine, hosts.find((row) => row.id === engine.id) ?? null);
    return gap && gap.kind !== "not_found" ? [{ engine, gap }] : [];
  });
  if (rows.length === 0) return null;
  return (
    <>
      <div className="set-grp">Needs attention <span className="set-pill planned dot">Planned</span></div>
      <p className="setting-line-desc eh-caption">Engines that can’t run yet, and why. Each row says one thing and offers the one action that can change it.</p>
      <div className="set-tbl eh-table" role="table" aria-label="Needs attention">
        {rows.map(({ engine, gap }) => (
          <div key={engine.id} role="row" className="set-tr">
            <span role="cell"><span className="set-mark">{plain(engine.name).charAt(0).toUpperCase()}</span></span>
            <span role="cell"><span className="name">{plain(engine.name)}</span><br /><span className="sub eh-sub">{gap.sentence}</span></span>
            <span role="cell" className="eh-chips"><HostPill chip={gap.chip} /></span>
            <span role="cell">
              {gap.action === "open" ? <Button small onClick={() => onOpen(engine.id)}>Open</Button> : null}
              {gap.action === "sign_in_again" ? <Button small onClick={() => onOpen(engine.id)}>Sign in again</Button> : null}
            </span>
          </div>
        ))}
      </div>
    </>
  );
}

/**
 * The card on a provider's detail page for what stops this engine: the SDK missing from the build (with a Rust driver to fall back
 * to, or without one) or an expired sign-in. Sign in with the engine's own command, shown in the Login row above; the app opens
 * nothing here.
 */
export function EngineGapCard({ engine }: { engine: EngineStatus }) {
  const gap = useGap(engine);
  if (!gap) return null;
  const name = plain(engine.name, 60);
  return (
    <section className="setting-group" role="group" aria-label="Needs attention">
      <h2>{gap.kind === "expired" ? "Login" : `${name} SDK missing in this build`}</h2>
      <div className="setting-list">
        {gap.kind === "sdk_missing" ? <SettingRow title="What runs" description={`${name} runs through its ${programOf(engine)} program. Each run says “Running without the engine host”.`}><HostPill chip={gap.chip} /></SettingRow> : null}
        {gap.kind === "unavailable" ? <>
          <SettingRow title="State" description={gap.sentence}><HostPill chip={gap.chip} /></SettingRow>
          <SettingRow title="Send" description={`Off for ${name} threads. ${name} can’t run here.`} />
        </> : null}
        {gap.kind === "expired" ? <SettingRow title="Login" description={gap.sentence}><HostPill chip={gap.chip} /></SettingRow> : null}
      </div>
    </section>
  );
}
