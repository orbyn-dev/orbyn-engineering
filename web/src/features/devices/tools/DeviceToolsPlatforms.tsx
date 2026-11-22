import { Bell, Eye, MapPin, Shield, Smartphone, Sun, Type } from "lucide-react";
import type { ReactNode } from "react";

interface Row {
  name: string;
  note: string;
  icon: ReactNode;
  ios: string;
  android: string;
}

const ROWS: readonly Row[] = [
  { name: "Appearance", note: "Light and Dark, content size", icon: <Sun size={14} aria-hidden />, ios: "iOS · Not verified", android: "Android · Not verified" },
  { name: "Text size", note: "Dynamic Type or font scale", icon: <Type size={14} aria-hidden />, ios: "iOS · Not verified", android: "Android · Not verified" },
  { name: "Accessibility", note: "Outline elements and read the tree", icon: <Eye size={14} aria-hidden />, ios: "iOS · Not verified", android: "Android · Not verified" },
  { name: "Location", note: "A simulated place", icon: <MapPin size={14} aria-hidden />, ios: "iOS · Not verified", android: "Android · Not verified" },
  { name: "Permissions", note: "Grant, deny and reset per app", icon: <Shield size={14} aria-hidden />, ios: "iOS · Not verified", android: "Android · Not verified" },
  { name: "Push", note: "A test notification", icon: <Bell size={14} aria-hidden />, ios: "iOS · Not verified", android: "Not available on Android" },
  { name: "Hardware keys", note: "Back and Recents", icon: <Smartphone size={14} aria-hidden />, ios: "Not available on iOS", android: "Android · Not verified" },
];

/**
 * The table the drawer's tabs promise (device-tools-platforms). It is a statement of what each platform is expected
 * to support, not a claim that any of it works: every line that is not "Not available" says Not verified.
 */
export function DeviceToolsPlatforms() {
  return (
    <div className="dt-platforms">
      <div className="dt-platforms-head">
        <h3>Device tools by platform</h3>
        <p>Not verified means Orbyn has not yet seen this work on a real device.</p>
      </div>
      <div className="dt-platform-list">
        {ROWS.map((row) => (
          <div className="dt-row" key={row.name}>
            <span className="dt-row-icon" aria-hidden>{row.icon}</span>
            <div className="dt-row-label"><b>{row.name}</b><small>{row.note}</small></div>
            <div className="dt-platform-side">
              <span className={row.ios === "Not available on iOS" ? "dt-status" : "dt-status dt-status-verified"}>{row.ios}</span>
              <span className={row.android === "Not available on Android" ? "dt-status" : "dt-status dt-status-verified"}>{row.android}</span>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
