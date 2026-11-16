import "./frame.css";
import "./design.css";
import type { ReactNode } from "react";

/**
 * One drawn screen from the Crew designs of v20 to v23. `width` is a narrow-window frame, `panel` the 320 px side
 * panel, `sheet` a Settings page and `height` the stage a dialog is centred in. The notice stays on every frame: these
 * screens run on drawn sample data and nothing in them reaches a repository.
 */
export function Frame({
  width,
  height,
  panel,
  sheet,
  children,
}: {
  width?: number;
  height?: number;
  panel?: boolean;
  sheet?: boolean;
  children: ReactNode;
}) {
  const className = ["cx-frame", panel ? "cx-panel" : "", sheet ? "cx-sheetpage" : ""].filter(Boolean).join(" ");
  return (
    <div className="cx-stage">
      <p className="cx-notice" role="note">
        <strong>Sample data.</strong> Crew is not connected to this app yet, so nothing here changes a repository.
      </p>
      <div className={className} style={{ width: panel ? 320 : width, minHeight: height }}>
        {children}
      </div>
    </div>
  );
}
