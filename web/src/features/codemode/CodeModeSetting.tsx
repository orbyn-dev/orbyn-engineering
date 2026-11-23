import { useEffect, useRef, useState } from "react";
import "./codemode.css";
import type { EngineStatus } from "../../api/types";
import { toFailure } from "../../lib/useAsync";
import { getCodeMode, putCodeMode } from "./api";

/**
 * Settings › Providers › an engine: the Code Mode group. One switch, off by default, per engine. It writes only from a click
 * (one click is one PUT) and is read once when the page opens. The service answers 404 on both routes when Code Mode is off
 * (`ORBYN_CODE_MODE` unset): that is "not offered", and the switch stays off and cannot be pressed, with the Planned pill and no
 * error. The pill goes only once the service has answered the read.
 *
 * The markup is the settings rows' own class names, so this feature needs no import from the settings feature.
 */
export function CodeModeSetting({ engine }: { engine: EngineStatus }) {
  const [on, setOn] = useState<boolean | null>(null);
  /** null until the service answered; false on a 404; true once it answered with a value. */
  const [offered, setOffered] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const writing = useRef(false);
  const current = useRef(engine.id);
  current.current = engine.id;
  const codex = (engine.instance_of ?? engine.id) === "codex";

  useEffect(() => {
    const stop = new AbortController();
    setOn(null);
    setOffered(null);
    // A write still out for the engine this page just left does not hold this one.
    writing.current = false;
    setBusy(false);
    setFailure(null);
    getCodeMode(engine.id, stop.signal)
      .then((stored) => {
        if (stop.signal.aborted) return;
        setOffered(stored !== null);
        setOn(stored?.enabled === true);
      })
      .catch((reason: unknown) => {
        if (stop.signal.aborted) return;
        setOn(false);
        setFailure(`Couldn't read this setting. ${toFailure(reason).message}`);
      });
    return () => stop.abort();
  }, [engine.id]);

  const flip = async () => {
    // One click, one write: a second press while the first is out does nothing, and nothing is written until the service answered the read.
    if (writing.current || on === null || offered !== true) return;
    writing.current = true;
    setBusy(true);
    setFailure(null);
    const target = engine.id;
    try {
      const stored = await putCodeMode(target, !on);
      // The page may have moved to another engine while the write was out: its answer is not this engine's.
      if (current.current === target) {
        // A 404 here means the service stopped offering Code Mode since the read.
        if (stored === null) {
          setOffered(false);
          setOn(false);
        } else {
          setOn(stored.enabled === true);
        }
      }
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
        {offered === true ? null : <span className="set-pill planned dot">Planned</span>}
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
              <input type="checkbox" role="switch" aria-label="Use Code Mode" checked={on === true} disabled={offered !== true || busy || on === null} onChange={() => void flip()} />
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
