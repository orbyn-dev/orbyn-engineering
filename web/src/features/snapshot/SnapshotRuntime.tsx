import { useEffect } from "react";
import { useSnapshot } from "./runtime";
import { SnapshotSetupDialog } from "./SnapshotSetupDialog";
import "./snapshot.css";

/**
 * Mounts the Snap Shot runtime once in the app. It draws the setup dialog (from Settings or the first enable), the 120 ms
 * flash, and nothing else; the settings group and the composer button decide their own visibility from the same store.
 */
export function SnapshotRuntime() {
  const start = useSnapshot((state) => state.start);
  const flashing = useSnapshot((state) => state.flashing);
  useEffect(() => {
    void start();
  }, [start]);
  return (
    <>
      <SnapshotSetupDialog />
      {flashing ? <div className="snapshot-flash" aria-hidden /> : null}
    </>
  );
}
