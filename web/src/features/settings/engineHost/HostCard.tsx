import { SettingRow } from "../rows/SettingRow";
import { SettingsBanner } from "../rows/SettingsBanner";
import type { HostRow } from "./contract";
import { useEngineHosts } from "./hostApi";
import { bannerLine, capabilityLine, stateLine } from "./hostText";
import { CauseChain, DetailsToggle, HostChip, useToggle } from "./HostParts";

function useHostRow(engineId: string): HostRow | null {
  return useEngineHosts().hosts?.find((row) => row.id === engineId) ?? null;
}

/** The banner above a provider's detail page when its host crashed or does not match the app. */
export function EngineHostBanner({ engineId }: { engineId: string }) {
  const row = useHostRow(engineId);
  const line = row ? bannerLine(row) : null;
  return line ? <SettingsBanner tone="err">{line}</SettingsBanner> : null;
}

const mono = (text: string) => <span className="mono">{text}</span>;

/**
 * The Engine host group of a provider's detail page, one state at a time. Only a crash or a failed start adds the
 * last error, and only that error has Show details. A stack is never shown here.
 */
export function EngineHostCard({ engineId }: { engineId: string }) {
  const row = useHostRow(engineId);
  const [open, toggle] = useToggle();
  if (!row) return null;
  const error = row.last_error;
  const failed = row.state === "crashed" || row.state === "restarting" || row.state === "cant_start";
  const hello = row.state === "running";
  return (
    <section className="setting-group" role="group" aria-label="Engine host">
      <h2>Engine host</h2>
      <div className="setting-list">
        <SettingRow title="State" description={stateLine(row)}><HostChip state={row.state} /></SettingRow>
        {hello || row.state === "version_mismatch" ? (
          <SettingRow title="Host" description={<>{mono(`orbyn-engine-host ${row.host_version ?? ""}`)}{row.protocol != null ? ` · protocol ${row.protocol}` : ""}</>} />
        ) : null}
        {row.state === "version_mismatch" && row.app_protocol != null ? <SettingRow title="This app" description={`Protocol ${row.app_protocol}`} /> : null}
        {hello && row.sdk ? <SettingRow title="Engine SDK" description={<>{mono(row.sdk.name)} {row.sdk.version}</>} /> : null}
        {hello && row.capabilities && row.capabilities.length > 0 ? <SettingRow title="It can" description={capabilityLine(row.capabilities)} /> : null}
        {failed && error ? <SettingRow title="Last error" description={error.message}>
          {error.causes.length > 0 || error.kind ? <DetailsToggle open={open} onToggle={toggle} /> : null}
        </SettingRow> : null}
      </div>
      {failed && error && open ? <CauseChain kind={error.kind} causes={error.causes} /> : null}
    </section>
  );
}
