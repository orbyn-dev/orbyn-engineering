import { ArrowLeft, Loader2 } from "lucide-react";
import { useState } from "react";
import { useAsync } from "../../../lib/useAsync";
import { Button, Pill } from "../../../ui/controls";
import { plain } from "../model";
import { devicesPlannedApi, usePlannedApiVersion } from "../planned";
import { ToolRow, ToolStepsView } from "./ToolRow";

/**
 * Tool versions (H31): each tool's installed and latest tested version. The service has no version check, so in
 * production this page says it is Planned and shows no rows. The mock answers Ready, Update available and Not
 * checked (offline). Orbyn downloads nothing: the vendor's steps open here and Check now re-reads.
 */
export function ToolVersions({ onBack, initialOpenSteps = [] }: { onBack(): void; initialOpenSteps?: readonly string[] }) {
  const seen = usePlannedApiVersion();
  const read = useAsync(`device-versions:${seen}`, (signal) => devicesPlannedApi().versions(signal));
  const [openSteps, setOpenSteps] = useState<ReadonlySet<string>>(() => new Set(initialOpenSteps));
  const versions = read.status === "ready" ? read.data : null;
  const planned = read.status === "ready" && versions === null;
  const checking = read.status === "loading" || read.refreshing;

  return (
    <div className="dev-versions">
      <div className="dev-setup-head">
        <div>
          <h3 className="dev-setup-title">Tool versions</h3>
          {versions && !versions.offline ? <p className="dev-note">Checked when the Devices tab opens and once a day. Orbyn compares each tool with the version it was tested with.</p> : null}
        </div>
        {read.status === "error" ? null : (
          <Button small tone="quiet" onClick={read.reloadNow} disabled={checking || planned}>{checking ? "Checking…" : versions?.checkedAt ? "Check again" : "Check now"}</Button>
        )}
      </div>
      {read.status === "error" ? (
        <div className="dev-fail" role="alert">
          <span>Could not check for updates. {plain(read.error.message, 200)}</span>
          <Button small tone="quiet" onClick={read.reloadNow}>Try again</Button>
        </div>
      ) : null}
      {checking ? <p className="dev-note" role="status"><Loader2 size={14} className="spin" aria-hidden /> Checking…</p> : null}
      {planned ? (
        <p className="dev-planned-line" role="status"><Pill>Planned</Pill> <span>Tool versions are not available on this service yet. Orbyn shows the versions it finds once the service has a check; it never downloads or installs a toolchain.</span></p>
      ) : null}
      {versions?.offline ? <p className="dev-setup-message" role="status">Could not check for updates. You are offline, so these are the versions found on this computer.</p> : null}
      {versions && !versions.offline && versions.tools.some((tool) => tool.state === "update") ? (
        <p className="dev-setup-message" role="status">
          {`The installed version of ${versions.tools.filter((tool) => tool.state === "update").map((tool) => plain(tool.name, 80)).join(", ")} is older than the version Orbyn was tested with. Devices still start; some screens may look different.`}
        </p>
      ) : null}
      {versions ? (
        <>
          <ul className="dev-tool-list" aria-label="Tool versions">
            {versions.tools.map((tool) => (
              <ToolRow key={tool.id} tool={tool}>
                {tool.steps ? (
                  <Button small tone="quiet" onClick={() => setOpenSteps((held) => { const next = new Set(held); if (next.has(tool.id)) next.delete(tool.id); else next.add(tool.id); return next; })}>{openSteps.has(tool.id) ? "Hide steps" : "Show steps"}</Button>
                ) : null}
              </ToolRow>
            ))}
          </ul>
          {versions.tools.filter((tool) => openSteps.has(tool.id)).map((tool) => <ToolStepsView key={tool.id} tool={tool} />)}
          <p className="dev-note">{versions.checkedAt ? `Checked ${plain(versions.checkedAt, 40)}` : "Not checked yet"}</p>
        </>
      ) : null}
      <div className="dev-setup-foot">
        <Button small tone="quiet" icon={<ArrowLeft size={14} aria-hidden />} onClick={onBack}>Back to devices</Button>
      </div>
    </div>
  );
}
