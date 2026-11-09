import "./Logo.css";

/**
 * The Orbyn Engineering mark (Engineer Designs, Brand). Ring and prompt take `currentColor`; the dot
 * takes `--logo-accent`. 16, 20 and 24 are drawn on the pixel grid and are never scaled from the large
 * file; every other size uses the 512 drawing.
 */
export type LogoSize = 16 | 20 | 24 | 28 | 48 | 64;

type Grid = {
  ring: [cx: number, cy: number, r: number];
  prompt: string;
  /** x1, x2, y of the underscore. */
  line: [number, number, number];
  /** Ring stroke, prompt stroke. */
  weight: [number, number];
  /** Dot cx, cy, r. */
  dot: [number, number, number];
};

const GRID: Record<16 | 20 | 24, Grid> = {
  16: { ring: [8, 8, 6], prompt: "5.6,5.6 7.9,7.8 5.6,10", line: [9.2, 10.8, 10], weight: [2, 1.4], dot: [12.24, 3.76, 2.4] },
  20: { ring: [10, 10, 8], prompt: "6.9,6.8 9.6,9.5 6.9,12.2", line: [11.4, 13.4, 12.2], weight: [2, 1.7], dot: [15.66, 4.34, 2.8] },
  24: { ring: [12, 12, 9.75], prompt: "8.2,8.2 11.6,11.6 8.2,15", line: [13.6, 16.2, 15], weight: [2.5, 2], dot: [18.89, 5.11, 3.3] },
};

const isGrid = (size: LogoSize): size is 16 | 20 | 24 => size === 16 || size === 20 || size === 24;

const DOT = "var(--logo-accent)";
const LINE = { fill: "none", stroke: "currentColor", strokeLinecap: "round", strokeLinejoin: "round" } as const;

export function Logo({ size, className }: { size: LogoSize; className?: string }) {
  const classes = className ? `logo ${className}` : "logo";
  if (isGrid(size)) {
    const g = GRID[size];
    return (
      <svg className={classes} width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden="true" focusable="false" data-logo={size}>
        <circle cx={g.ring[0]} cy={g.ring[1]} r={g.ring[2]} {...LINE} strokeWidth={g.weight[0]} />
        <polyline points={g.prompt} {...LINE} strokeWidth={g.weight[1]} />
        <line x1={g.line[0]} y1={g.line[2]} x2={g.line[1]} y2={g.line[2]} {...LINE} strokeWidth={g.weight[1]} />
        <circle cx={g.dot[0]} cy={g.dot[1]} r={g.dot[2]} fill={DOT} />
      </svg>
    );
  }
  return (
    <svg className={classes} width={size} height={size} viewBox="0 0 512 512" aria-hidden="true" focusable="false" data-logo={size}>
      <circle cx="256" cy="256" r="124.5" {...LINE} strokeWidth={29} />
      <polyline points="202,204 262,254 202,304" {...LINE} strokeWidth={29} />
      <line x1="286" y1="304" x2="322" y2="304" {...LINE} strokeWidth={29} />
      <circle cx="344.03" cy="167.97" r="31" fill={DOT} />
    </svg>
  );
}

/** The mark on its tile, in the theme's light or dark variant: 40 px with a 28 px mark, or 28 px with a 20 px mark. */
export function LogoTile({ size }: { size: 28 | 40 }) {
  return (
    <span className="logo-tile" data-size={size}>
      <Logo size={size === 40 ? 28 : 20} />
    </span>
  );
}
