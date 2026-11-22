import type { ReactNode } from "react";
import { CopyCommand, Pill } from "../../../ui/controls";
import type { DeviceTool, ToolState } from "../planned";
import { plain, safeHref } from "../model";

/** The chip each tool state draws. Red is only for a failed install. */
export function toolChip(state: ToolState): { label: string; tone: "neutral" | "accent" | "warn" | "danger" | "info" } {
  switch (state) {
    case "ready": return { label: "Ready", tone: "info" };
    case "missing": return { label: "Missing", tone: "warn" };
    case "update": return { label: "Update available", tone: "warn" };
    case "failed": return { label: "Failed", tone: "danger" };
    case "checking": return { label: "Checking…", tone: "neutral" };
    case "not_checked": return { label: "Not checked", tone: "neutral" };
  }
}

export function ToolChip({ state }: { state: ToolState }) {
  const chip = toolChip(state);
  return <Pill tone={chip.tone}>{chip.label}</Pill>;
}

/** The vendor's steps, with the command to copy and the official page with its domain. Nothing here runs an update. */
export function ToolStepsView({ tool }: { tool: DeviceTool }) {
  const steps = tool.steps;
  if (!steps) return null;
  const href = safeHref(steps.url);
  return (
    <div className="dev-tool-steps">
      <b>{plain(steps.title, 100)}</b>
      <p>{plain(steps.text, 300)}</p>
      {steps.command ? <CopyCommand command={steps.command} label="Copy command" /> : null}
      {href ? (
        <p className="dev-tool-link">
          <a href={href} target="_blank" rel="noreferrer">{plain(steps.domain ?? href, 80)} · opens in your browser</a>
          <span className="dev-tool-example">Example</span>
        </p>
      ) : null}
      <p className="dev-note">Orbyn does not download or install this. Update it with the vendor’s tool, then check again.</p>
    </div>
  );
}

/** One tool row: its name, version or note, and its state chip. */
export function ToolRow({ tool, children }: { tool: DeviceTool; children?: ReactNode }) {
  return (
    <li className="dev-tool-row" data-state={tool.state}>
      <div className="dev-tool-name">
        <b>{plain(tool.name, 80)}</b>
        <span className="dev-tool-note">
          {plain(tool.note ?? (tool.version ? `Version ${tool.version}` : "Not found on this computer"), 160)}
        </span>
      </div>
      <div className="dev-tool-side">
        <ToolChip state={tool.state} />
        {children}
      </div>
    </li>
  );
}
