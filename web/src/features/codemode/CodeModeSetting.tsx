import { useEffect, useRef, useState } from "react";
import "./codemode.css";
import type { EngineStatus } from "../../api/types";
import { toFailure } from "../../lib/useAsync";
import { codeModeLive, getCodeMode, putCodeMode } from "./api";

/**
 * Settings › Providers › an engine: the Code Mode group. One switch, off by default, per engine. It writes only from a click
 * (one click is one PUT) and is read once when the page opens. While the service has no route for it (a production build,
 * `codeModeLive` false) the switch is shown off and cannot be pressed: nothing is read or written. The Planned pill stays
 * until Code Mode ships.
 *
 * The markup is the settings rows' own class names, so this feature needs no import from the settings feature.
 */
export function CodeModeSetting({ engine }: { engine: EngineStatus }) {
  const [on, setOn] = useState<boolean | null>(codeModeLive ? null : false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const writing = useRef(false);
  const current = useRef(engine.id);
  current.current = engine.id;
  const codex = (engine.instance_of ?? engine.id) === "codex";

  useEffect(() => {
    if (!codeModeLive) return;
    const stop = new AbortController();
    setOn(null);
    // A write still out for the engine this page just left does not hold this one.
    writing.current = false;
    setBusy(false);
    setFailure(null);
    getCodeMode(engine.id, stop.signal)
      .then((stored) => setOn(stored.enabled === true))
      .catch((reason: unknown) => {
        if (stop.signal.aborted) return;
        setOn(false);
        setFailure(`Couldn't read this setting. ${toFailure(reason).message}`);
      });
    return () => stop.abort();
  }, [engine.id]);

  const flip = async () => {
    // One click, one write: a second press while the first is out does nothing.
    // Dead code in a production build, where there is no route to write to.
    if (!codeModeLive || writing.current || on === null) return;
    writing.current = true;
    setBusy(true);
    setFailure(null);
    const target = engine.id;
    try {
      const stored = await putCodeMode(target, !on);
      // The page may have moved to another engine while the write was out: its answer is not this engine's.
      if (current.current === target) setOn(stored.enabled === true);
    } catch (reason) {
      if (current.current === target) setFailure(`${toFailure(reason).message} Try again.`);
    } finally {
      if (current.current === target) {
        writing.current = false;
        setBusy(false);
      }
    }
  };

  const description = `Lets this engine write one short script that calls many Orbyn tools. Every call in the script still asks as usual.${codex ? " Codex also has its own code mode. Both work, and neither needs the other." : ""}`;
  return (
    <>
      <p className="set-grp">
        Code Mode
        <span className="set-pill planned dot">Planned</span>
      </p>
      <section className="setting-group">
        <div className="setting-list">
          <div className="setting-line">
            <div className="setting-line-text">
              <div className="setting-line-heading">
                <span className="setting-line-title">Use Code Mode</span>
              </div>
              <span className="setting-line-desc">{description}</span>
            </div>
            <div className="setting-line-control">
              <input type="checkbox" role="switch" aria-label="Use Code Mode" checked={on === true} disabled={!codeModeLive || busy || on === null} onChange={() => void flip()} />
            </div>
          </div>
          <div className="setting-line">
            <div className="setting-line-text">
              <span className="setting-line-desc cm-set-note">
                {on ? (
                  <>
                    Listed to this engine: <span className="mono">orbyn_code_search</span>, <span className="mono">orbyn_code_describe</span> and <span className="mono">orbyn_code_run</span>. Approvals, rules and Autopilot work as for any other call.
                  </>
                ) : (
                  "Off by default. The three tools are not listed to this engine."
                )}
              </span>
            </div>
          </div>
        </div>
      </section>
      {failure ? <p className="field-error" role="alert">{failure}</p> : null}
    </>
  );
}
