import { Apple, ArrowLeft, Check, Loader2, Smartphone } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { useAsync } from "../../../lib/useAsync";
import { Button, Pill } from "../../../ui/controls";
import type { Device, Platform, PlatformStatus } from "../contract";
import { installPercent, plain } from "../model";
import { devicesPlannedApi, usePlannedApiVersion, type DeviceTool, type InstallProgress } from "../planned";
import { ToolRow, ToolStepsView } from "./ToolRow";

type Step = 1 | 2 | 3 | "done";

const STEPS: { step: 1 | 2 | 3; label: string }[] = [
  { step: 1, label: "Platform" },
  { step: 2, label: "Tools" },
  { step: 3, label: "Agent access" },
];

const macHost = () => typeof navigator !== "undefined" && /Mac|iPhone|iPad/i.test(navigator.userAgent);

const openLabel = (tool: DeviceTool): string => (tool.state === "ready" ? "Hide steps" : "Show steps");

/**
 * The setup hub (H21): Platform, Tools, Agent access, then a short ready summary. Orbyn installs no toolchain:
 * the vendor's steps are shown and Check again re-reads. The one install is agent-device; it and the access toggle
 * come from the Planned client, so in production the hub says that the checks are Planned and Continue stays shut.
 * Auto-opened when the service reports no available platform; reachable later from the device tab menu.
 */
export function SetupHub({ platforms, deviceToOpen, initialStep = 1, initialPlatform, initialInstall = null, host, onOpen, onClose }: {
  platforms: readonly PlatformStatus[];
  deviceToOpen: Device | null;
  initialStep?: Step;
  initialPlatform?: Platform;
  /** The preview install frame; production and tests start from nothing. */
  initialInstall?: InstallProgress | null;
  /** The preview can draw the Windows frame on a Mac; production reads the host. */
  host?: "mac" | "other";
  onOpen(deviceId: string | null): void;
  onClose(): void;
}) {
  const mac = host ? host === "mac" : macHost();
  const [step, setStep] = useState<Step>(initialStep);
  const [platform, setPlatform] = useState<Platform | null>(initialPlatform ?? (mac ? "ios" : "android"));
  const [access, setAccess] = useState(false);
  const [accessBusy, setAccessBusy] = useState(false);
  const [accessPlanned, setAccessPlanned] = useState(false);
  const [checking, setChecking] = useState(false);
  const [checkError, setCheckError] = useState("");
  const [install, setInstall] = useState<InstallProgress | null>(initialInstall);
  const [installing, setInstalling] = useState(false);
  const [openSteps, setOpenSteps] = useState<ReadonlySet<string>>(new Set());
  const seen = usePlannedApiVersion();
  const read = useAsync(`device-setup:${seen}:${platform ?? "none"}`, (signal) => (platform ? devicesPlannedApi().tools(platform, signal) : Promise.resolve(null)));
  const tools = read.status === "ready" ? read.data : null;
  const loading = read.status === "loading";
  const failed = read.status === "error" ? plain(read.error.message, 200) : "";
  const planned = read.status === "ready" && tools === null;
  const status = platform ? platforms.find((one) => one.platform === platform) : undefined;
  const platformMessage = status?.available === false ? plain(status.message, 200) : "";
  const ios = mac;
  const keep = useRef(access);
  keep.current = access;

  useEffect(() => {
    if (tools) setAccess(tools.access);
  }, [tools]);

  const firstPlatform = useRef(platform);
  useEffect(() => {
    if (firstPlatform.current === platform) return;
    setInstall(null);
    setCheckError("");
    setOpenSteps(new Set());
  }, [platform]);

  const listed = tools?.tools ?? [];
  const required = listed.filter((tool) => tool.required);
  const requiredReady = required.length > 0 && required.every((tool) => tool.state === "ready");
  const readyCount = listed.filter((tool) => tool.state === "ready").length;
  const optional = listed.some((tool) => !tool.required);
  const agent = listed.find((tool) => tool.id === "agent-device");
  const canContinue = platform !== null && requiredReady && !loading && !planned && !failed;

  const checkAgain = async () => {
    if (!platform || checking) return;
    setChecking(true);
    setCheckError("");
    try {
      const answer = await devicesPlannedApi().check(platform);
      if (answer === null) setCheckError("Tool checks are Planned. The service has no such route yet.");
      else read.setData(answer);
    } catch (error) {
      setCheckError(error instanceof Error ? plain(error.message, 200) : "Couldn’t check the tools.");
    } finally {
      setChecking(false);
    }
  };

  const installAgent = async () => {
    if (!platform || installing) return;
    setInstalling(true);
    setInstall({ state: "installing", done: 0, total: 0, line: "Starting…", lines: [] });
    try {
      const answer = await devicesPlannedApi().install(platform, (progress) => setInstall(progress));
      if (answer === null) setCheckError("Installing agent-device is Planned. The service has no such route yet.");
      else read.setData(answer);
    } catch (error) {
      setInstall((current) => current?.state === "failed" ? current : {
        state: "failed",
        done: 0,
        total: 0,
        line: error instanceof Error ? plain(error.message, 160) : "The install failed.",
        error: error instanceof Error ? plain(error.message, 160) : "The install failed.",
        lines: [],
      });
    } finally {
      setInstalling(false);
    }
  };

  const toggleAccess = async (on: boolean) => {
    if (accessBusy) return;
    setAccessBusy(true);
    setAccessPlanned(false);
    try {
      const answer = await devicesPlannedApi().setAccess(on);
      if (answer === null) {
        setAccess(false);
        setAccessPlanned(true);
      } else {
        setAccess(answer);
        if (tools) read.setData({ ...tools, access: answer });
      }
    } catch {
      setAccess(keep.current);
    } finally {
      setAccessBusy(false);
    }
  };

  const chooseStep = (next: 1 | 2 | 3) => {
    if (step === "done" || next > step) return;
    setStep(next);
  };

  if (step === "done") {
    const summary = platform === "ios" ? "iOS Simulator is set up on this Mac." : platform === "android" ? "Android Emulator is set up on this computer." : "Devices are set up.";
    return (
      <div className="dev-setup">
        <Stepper step={step} onStep={chooseStep} />
        <section className="dev-setup-body" aria-label="Devices are ready">
          <h3 className="dev-setup-title">Devices are ready</h3>
          <p className="dev-note">{summary}</p>
          <ul className="dev-tool-list">
            {listed.filter((tool) => tool.required).map((tool) => <ToolRow key={tool.id} tool={tool} />)}
          </ul>
          <div className="dev-access-summary">
            <b>Agent access</b>
            <span className="dev-note">{access ? "On. Agents can use devices when a run asks." : "Off. Agents cannot use devices."}</span>
            <Button small tone="quiet" onClick={() => setStep(3)}>Change</Button>
          </div>
          <div className="dev-setup-actions">
            <Button onClick={() => onOpen(deviceToOpen?.id ?? null)}>{deviceToOpen ? `Open ${plain(deviceToOpen.name, 40)}` : "Open Devices"}</Button>
          </div>
        </section>
      </div>
    );
  }

  return (
    <div className="dev-setup">
      <Stepper step={step} onStep={chooseStep} />
      {step === 1 ? (
        <section className="dev-setup-body" aria-label="Choose a platform">
          <h3 className="dev-setup-title">Choose a platform</h3>
          <p className="dev-note">Devices run on the computer that runs Orbyn’s service, {mac ? "this Mac" : "this computer"}. You can add the other platform later.</p>
          <div className="dev-platform-cards" role="radiogroup" aria-label="Platform">
            <button type="button" role="radio" aria-checked={platform === "ios"} className="dev-platform" disabled={!ios} onClick={() => setPlatform("ios")}>
              <Apple size={20} aria-hidden />
              <span className="dev-platform-name">iOS Simulator</span>
              <span className="dev-platform-desc">iPhone and iPad simulators from Xcode. Needs a Mac.</span>
              <span className="dev-platform-state">{ios ? (platform === "ios" ? "Available on this Mac" : "Available") : "Needs a Mac with Xcode. This computer runs Windows."}</span>
            </button>
            <button type="button" role="radio" aria-checked={platform === "android"} className="dev-platform" onClick={() => setPlatform("android")}>
              <Smartphone size={20} aria-hidden />
              <span className="dev-platform-name">Android Emulator</span>
              <span className="dev-platform-desc">Pixel and other virtual devices from the Android SDK.</span>
              <span className="dev-platform-state">Available</span>
            </button>
          </div>
          {!ios && platform === "ios" ? <p className="dev-note">iOS needs a Mac with Xcode. Android stays available on this computer.</p> : null}
          <div className="dev-setup-actions">
            <Button disabled={platform === null || (platform === "ios" && !ios)} onClick={() => setStep(2)}>Continue</Button>
          </div>
        </section>
      ) : null}

      {step === 2 && platform ? (
        <section className="dev-setup-body" aria-label={`Tools for the ${platform === "ios" ? "iOS Simulator" : "Android Emulator"}`}>
          <div className="dev-setup-head">
            <div>
              <h3 className="dev-setup-title">Tools for the {platform === "ios" ? "iOS Simulator" : "Android Emulator"}</h3>
              <p className="dev-note">Orbyn checks what is installed. Required tools must be ready before a device can start.</p>
            </div>
            <Button small tone="quiet" onClick={() => void checkAgain()} disabled={checking || installing}>{checking ? "Checking…" : "Check again"}</Button>
          </div>
          {platformMessage ? <p className="dev-setup-message">{platformMessage}</p> : null}
          {loading ? <p className="dev-note" role="status"><Loader2 size={14} className="spin" aria-hidden /> Checking the tools…</p> : null}
          {planned ? (
            <p className="dev-planned-line" role="status"><Pill>Planned</Pill> <span>Tool checks and setup are not available on this service yet. Orbyn installs no toolchain; a missing tool is fixed with the vendor’s own installer.</span></p>
          ) : null}
          {failed ? <p className="field-error" role="alert">{failed}</p> : null}
          {checkError ? <p className="field-error" role="alert">{checkError}</p> : null}
          {tools ? (
            <>
              <ul className="dev-tool-list" aria-label="Tools">
                {listed.map((tool) => (
                  <ToolRow key={tool.id} tool={install?.state === "installing" && tool.id === "agent-device" ? { ...tool, state: "checking", version: install.done ? `${install.done} of ${install.total} KB` : tool.version } : tool}>
                    {tool.state !== "ready" && tool.steps ? (
                      <Button small tone="quiet" onClick={() => setOpenSteps((held) => { const next = new Set(held); if (next.has(tool.id)) next.delete(tool.id); else next.add(tool.id); return next; })}>{openSteps.has(tool.id) ? "Hide steps" : openLabel(tool)}</Button>
                    ) : null}
                    {tool.id === "agent-device" && tool.state !== "ready" && install?.state !== "installing" ? (
                      <Button small onClick={() => void installAgent()} disabled={installing}>{install?.state === "failed" ? "Try again" : "Install"}</Button>
                    ) : null}
                  </ToolRow>
                ))}
              </ul>
              {listed.filter((tool) => openSteps.has(tool.id)).map((tool) => <ToolStepsView key={tool.id} tool={tool} />)}
              {install ? <InstallPanel progress={install} onCopy={() => void navigator.clipboard?.writeText(install.lines.join("\n"))} /> : null}
              {agent && agent.state === "missing" ? <p className="dev-note">agent-device is optional: agents cannot use devices until it is installed.</p> : null}
              <p className="dev-note" role="status">{readyCount} of {listed.length} ready{optional ? " · agent-device is optional" : ""}</p>
            </>
          ) : null}
          <div className="dev-setup-actions">
            <Button tone="quiet" icon={<ArrowLeft size={14} aria-hidden />} onClick={() => setStep(1)}>Back</Button>
            <Button disabled={!canContinue} onClick={() => setStep(3)}>Continue</Button>
          </div>
        </section>
      ) : null}

      {step === 3 ? (
        <section className="dev-setup-body" aria-label="Agent access">
          <h3 className="dev-setup-title">Agent access</h3>
          <p className="dev-note">Decide whether agents can use devices. This is off until you turn it on. Off means agents cannot use devices at all. You can change it any time in this tab’s menu.</p>
          <div className="dev-access">
            <div>
              <b>Let agents use devices</b>
              <p className="dev-note">Agents can open a device, tap, type and take screenshots. They cannot install apps or change device settings without asking.</p>
            </div>
            <button
              type="button"
              role="switch"
              aria-checked={access}
              aria-label="Let agents use devices"
              className="dev-switch"
              disabled={accessBusy || planned}
              onClick={() => void toggleAccess(!access)}
            >
              <span className="dev-switch-knob" aria-hidden />
            </button>
          </div>
          <div className="dev-access">
            <div>
              <b>You can take over</b>
              <p className="dev-note">Touching the device pauses the agent until you hand it back.</p>
            </div>
          </div>
          {planned || accessPlanned ? <p className="dev-planned-line" role="status"><Pill>Planned</Pill> <span>Agent access is not available on this service yet; it always stays off.</span></p> : null}
          <div className="dev-setup-actions">
            <Button tone="quiet" icon={<ArrowLeft size={14} aria-hidden />} onClick={() => setStep(2)}>Back</Button>
            <Button onClick={() => setStep("done")} disabled={planned} icon={<Check size={14} aria-hidden />}>Finish</Button>
          </div>
        </section>
      ) : null}
      <div className="dev-setup-foot">
        <Button small tone="quiet" onClick={onClose}>Back to devices</Button>
      </div>
    </div>
  );
}

function Stepper({ step, onStep }: { step: Step; onStep(next: 1 | 2 | 3): void }) {
  return (
    <ol className="dev-steps" aria-label="Device setup steps">
      {STEPS.map(({ step: at, label }) => {
        const current = step === at || (step === "done" && at === 3);
        const done = step === "done" || (typeof step === "number" && at < step);
        return (
          <li key={at} className="dev-step" data-current={current || undefined} data-done={done || undefined}>
            <button type="button" className="dev-step-button" aria-current={current ? "step" : undefined} disabled={step === "done" || at > step} onClick={() => onStep(at)}>
              <span className="dev-step-no">{done ? <Check size={12} aria-hidden /> : at}</span>
              <span>{label}</span>
            </button>
          </li>
        );
      })}
    </ol>
  );
}

function InstallPanel({ progress, onCopy }: { progress: InstallProgress; onCopy(): void }) {
  const percent = installPercent(progress.done, progress.total);
  return (
    <div className="dev-install" data-state={progress.state} role={progress.state === "failed" ? "alert" : "status"}>
      <div className="dev-install-head">
        <b>{progress.state === "failed" ? "Could not install agent-device" : progress.state === "done" ? "agent-device installed" : "Installing agent-device"}</b>
        <span className="dev-note">{progress.total > 0 ? `${percent}%` : ""}</span>
      </div>
      {progress.state === "installing" ? <div className="dev-progress" role="progressbar" aria-label="Install progress" aria-valuemin={0} aria-valuemax={100} aria-valuenow={percent}><span style={{ width: `${percent}%` }} /></div> : null}
      <p className="dev-note">{plain(progress.line, 200)}</p>
      {progress.lines.length > 0 ? <pre className="dev-install-log" aria-label="Install log">{progress.lines.map((line) => plain(line, 300)).join("\n")}</pre> : null}
      <div className="dev-install-actions">
        <Button small tone="quiet" onClick={onCopy}>Copy log</Button>
      </div>
    </div>
  );
}
