import type { ReactNode } from "react";
import { Act } from "./Parts";

export interface BannerAction {
  label: string;
  primary?: boolean;
}

/** A strip under the toolbars: amber needs you, red failed, plain explains. Its actions all write, so each is guarded. */
export function Banner({
  tone,
  heading,
  text,
  actions,
  icon,
}: {
  tone: "amber" | "red" | "plain";
  heading?: string;
  text?: string;
  actions?: BannerAction[];
  icon?: ReactNode;
}) {
  return (
    <div className={`crew-bn crew-bn-${tone}`} role={tone === "plain" ? "status" : "alert"}>
      {icon}
      <span>
        {heading ? <b>{heading}</b> : null}
        {heading && text ? " " : null}
        {text}
      </span>
      {actions?.length ? (
        <span className="crew-bn-acts">
          {actions.map((action) => (
            <Act key={action.label} label={action.label} primary={action.primary} />
          ))}
        </span>
      ) : null}
    </div>
  );
}
