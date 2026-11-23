import { NETWORK_ERROR, type ApiFailure } from "../../../api/client";
import { Ic } from "../shared/parts";

/**
 * Couldn't read the history: in plain words, that nothing was changed, with Try again. Red is only for the failure. The tiles below stay empty rather
 * than showing old numbers, and the error's own text is under the banner for the person who needs it.
 */
export function Failure({ failure, onRetry }: { failure: ApiFailure; onRetry: () => void }) {
  const detail = failure.code === NETWORK_ERROR ? failure.message : `${failure.code}: ${failure.message}`;
  const copy = () => {
    try {
      void navigator.clipboard?.writeText(detail).catch(() => undefined);
    } catch {
      /* no clipboard here: the text stays visible below */
    }
  };
  return (
    <>
      <div className="cx-cwbn cx-red" style={{ margin: 0, flexWrap: "wrap" }} role="alert">
        <Ic n="alert" sm />
        <span style={{ flex: 1, minWidth: "200px" }}>Couldn’t read the run history on this device. Nothing was changed.</span>
        <span className="cx-acts">
          <button type="button" className="cx-ghost" onClick={copy}>Copy the error</button>
          <button type="button" className="cx-btn" onClick={onRetry}>Try again</button>
        </span>
      </div>
      <small className="cx-t10f" style={{ overflowWrap: "anywhere" }}>{detail.slice(0, 300)}</small>
    </>
  );
}
