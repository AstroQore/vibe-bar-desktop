/**
 * The SF Symbols the native popover draws, as inline SVG.
 *
 * WebKit has no SF Symbols, so each is a small path at the symbol's own
 * proportions, drawn in `currentColor` so the caller's text colour applies
 * exactly as `.foregroundStyle` does natively.
 */
import type { CSSProperties } from "react";

type Props = { size?: number; style?: CSSProperties; className?: string; title?: string };

function Svg({ size = 12, style, className, title, children, viewBox = "0 0 24 24" }: Props & { children: React.ReactNode; viewBox?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox={viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth={2.2}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={style}
      className={className}
      aria-hidden={title ? undefined : true}
      role={title ? "img" : undefined}
    >
      {title ? <title>{title}</title> : null}
      {children}
    </svg>
  );
}

/** chart.bar.fill */
export const ChartBar = (p: Props) => (
  <Svg {...p}><path fill="currentColor" stroke="none" d="M4 13h4v7H4zM10 4h4v16h-4zM16 9h4v11h-4z" /></Svg>
);
/** square.grid.2x2 */
export const Grid2x2 = (p: Props) => (
  <Svg {...p}><rect x="4" y="4" width="6.5" height="6.5" rx="1.4" /><rect x="13.5" y="4" width="6.5" height="6.5" rx="1.4" /><rect x="4" y="13.5" width="6.5" height="6.5" rx="1.4" /><rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.4" /></Svg>
);
/** server.rack */
export const ServerRack = (p: Props) => (
  <Svg {...p}><rect x="4" y="4" width="16" height="6" rx="1.5" /><rect x="4" y="14" width="16" height="6" rx="1.5" /><path d="M8 7h.01M8 17h.01" strokeWidth={3} /></Svg>
);
/** arrow.clockwise */
export const ArrowClockwise = (p: Props) => (
  <Svg {...p}><path d="M20 12a8 8 0 1 1-2.6-5.9" /><path d="M20 4v5h-5" /></Svg>
);
/** rectangle.on.rectangle */
export const RectOnRect = (p: Props) => (
  <Svg {...p}><rect x="8" y="8" width="13" height="10" rx="2" /><path d="M16 6V5a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h1" /></Svg>
);
/** macwindow */
export const MacWindow = (p: Props) => (
  <Svg {...p}><rect x="3" y="4" width="18" height="16" rx="2.5" /><path d="M3 9h18" /></Svg>
);
/** gearshape */
export const Gear = (p: Props) => (
  <Svg {...p}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></Svg>
);
/** checkmark.circle.fill */
export const CheckCircle = (p: Props) => (
  <Svg {...p}><circle cx="12" cy="12" r="10" fill="currentColor" stroke="none" /><path d="M8 12.5l2.8 2.8L16.5 9.5" stroke="var(--popover-on-fill, #fff)" strokeWidth={2.4} /></Svg>
);
/** exclamationmark.triangle.fill */
export const WarningTriangle = (p: Props) => (
  <Svg {...p}><path fill="currentColor" stroke="none" d="M12 3.2 22 20H2z" /><path d="M12 9v5M12 17.2h.01" stroke="var(--popover-on-fill, #fff)" strokeWidth={2.4} /></Svg>
);
/** xmark.octagon.fill */
export const XOctagon = (p: Props) => (
  <Svg {...p}><path fill="currentColor" stroke="none" d="M8 2h8l6 6v8l-6 6H8l-6-6V8z" /><path d="M9 9l6 6M15 9l-6 6" stroke="var(--popover-on-fill, #fff)" strokeWidth={2.4} /></Svg>
);
/** arrow.clockwise.circle.fill */
export const RefreshCircle = (p: Props) => (
  <Svg {...p}><circle cx="12" cy="12" r="10" fill="currentColor" stroke="none" /><path d="M16.5 12a4.5 4.5 0 1 1-1.5-3.3" stroke="var(--popover-on-fill, #fff)" strokeWidth={2} /><path d="M16.5 7.5v3h-3" stroke="var(--popover-on-fill, #fff)" strokeWidth={2} /></Svg>
);
/** wrench.and.screwdriver.fill */
export const Wrench = (p: Props) => (
  <Svg {...p}><path fill="currentColor" stroke="none" d="M14.7 6.3a4 4 0 0 0 5 5L9.5 21.5a2.1 2.1 0 0 1-3-3z" /><path d="M3 3l6 6" /></Svg>
);
/** clock.badge.exclamationmark */
export const ClockBadge = (p: Props) => (
  <Svg {...p}><circle cx="11" cy="13" r="8" /><path d="M11 8v5l3 2" /><circle cx="19" cy="6" r="3.5" fill="currentColor" stroke="none" /><path d="M19 4.3v1.9M19 7.9h.01" stroke="var(--popover-on-fill, #fff)" strokeWidth={1.6} /></Svg>
);
/** info.circle */
export const InfoCircle = (p: Props) => (
  <Svg {...p}><circle cx="12" cy="12" r="9" /><path d="M12 11v5M12 8h.01" /></Svg>
);
