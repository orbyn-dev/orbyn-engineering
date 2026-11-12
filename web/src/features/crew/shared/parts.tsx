import {
  Activity,
  Briefcase,
  Bug,
  Check,
  ChevronDown,
  ChevronRight,
  CircleCheck,
  Clock,
  Diff,
  Ellipsis,
  Eye,
  GitBranch,
  Folder,
  GitCommitHorizontal,
  Info,
  ListTree,
  Lock,
  Maximize2,
  Minus,
  Pencil,
  MessageSquare,
  Pause,
  Plus,
  RefreshCw,
  RotateCcw,
  Shield,
  Square,
  TriangleAlert,
  Workflow,
  X,
  User,
  type LucideIcon,
} from "lucide-react";
import type { CSSProperties, ReactNode } from "react";
import { unavailable } from "../autopilot/review/Parts";

/** The icons the Crew designs of v20 to v23 draw, by the name the design gives them. */
const ICONS = {
  activity: Activity,
  alert: TriangleAlert,
  branch: GitBranch,
  brief: Briefcase,
  bug: Bug,
  check2: Check,
  clock: Clock,
  diff: Diff,
  down: ChevronDown,
  edit: Pencil,
  eye: Eye,
  flow: Workflow,
  folder: Folder,
  git: GitCommitHorizontal,
  info: Info,
  lock: Lock,
  max: Maximize2,
  minus: Minus,
  more: Ellipsis,
  ok: CircleCheck,
  pause: Pause,
  plus: Plus,
  refresh: RefreshCw,
  restore: RotateCcw,
  right: ChevronRight,
  shield: Shield,
  side: MessageSquare,
  stop: Square,
  tree: ListTree,
  user: User,
  x: X,
} satisfies Record<string, LucideIcon>;

type IconName = keyof typeof ICONS;

export function Ic({ n, sm, className, style }: { n: IconName; sm?: boolean; className?: string; style?: CSSProperties }) {
  const Icon: LucideIcon = ICONS[n];
  return <Icon className={["cx-ic", sm ? "cx-sm" : "", className].filter(Boolean).join(" ")} size={sm ? 14 : 16} style={style} aria-hidden />;
}

/**
 * Every control in these screens is a write or a hand-off to a backend that does not exist yet, so each is a real
 * button, disabled, with the reason for everyone.
 */
export function Btn({
  className,
  style,
  children,
  ...rest
}: {
  className?: string;
  style?: CSSProperties;
  children?: ReactNode;
  "aria-label"?: string;
  title?: string;
}) {
  return (
    <button type="button" className={className} style={style} {...unavailable} {...rest}>
      {children}
    </button>
  );
}

/** A row of options with one drawn as chosen. It is display-only: nothing here changes a view. */
export function Seg({ kind, items, on, style }: { kind: "segx" | "cwseg"; items: string[]; on?: number; style?: CSSProperties }) {
  return (
    <span className={`cx-${kind}`} style={style}>
      {items.map((label, index) => (
        <span key={label} className={index === on ? "cx-on" : undefined} aria-current={index === on ? "true" : undefined}>
          {label}
        </span>
      ))}
    </span>
  );
}

/** The Crew page title as drawn on every screen. */
export function PageHead({ sub, acts, style }: { sub: string; acts?: boolean; style?: CSSProperties }) {
  return (
    <div className="cx-ph" style={style}>
      <h3>
        Crew<span className="cx-cwpv">Preview</span>
      </h3>
      <span className="cx-sub">{sub}</span>
      {acts ? (
        <div className="cx-acts">
          <Btn className="cx-ghost">
            <Ic n="pause" sm />
            Pause crew
          </Btn>
          <Btn className="cx-ghost">
            <Ic n="stop" sm />
            Stop crew
          </Btn>
        </div>
      ) : null}
    </div>
  );
}
