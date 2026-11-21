import { ArrowLeft, ChevronRight } from "lucide-react";
import { routeHref, type SettingsSection, type SettingsSubPage } from "../../product/state/route";
import { SUBPAGES } from "./subPage";

/**
 * Where the person is in Settings (parity K55, Engineer Designs pb-k55-*). On a wide window: Settings › Page [› Sub-page], the
 * ancestors links and the current page in bold, replacing the back link. On a narrow one a sub-page keeps "Back to Page" above the
 * title. Both only navigate.
 */
export function SettingsCrumbs({ section, label, sub, narrow, labelOf }: {
  section: SettingsSection; label: string; sub: SettingsSubPage | null; narrow: boolean; labelOf(section: SettingsSection): string;
}) {
  const page = routeHref({ name: "settings", section });
  if (sub && narrow) {
    return (
      <div className="settings-crumbs settings-crumbs-back">
        <a href={page}><ArrowLeft size={13} aria-hidden />Back to {labelOf(SUBPAGES[sub].parent)}</a>
      </div>
    );
  }
  return (
    <nav className="settings-crumbs" aria-label="Breadcrumb">
      <ol>
        <li><a href={routeHref({ name: "settings", section: "general" })}>Settings</a></li>
        {sub ? <li><ChevronRight size={12} aria-hidden /><a href={page}>{label}</a></li> : null}
        <li><ChevronRight size={12} aria-hidden /><b aria-current="page">{sub ? SUBPAGES[sub].label : label}</b></li>
      </ol>
    </nav>
  );
}
