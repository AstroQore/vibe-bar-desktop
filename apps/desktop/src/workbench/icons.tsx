/** The SF Symbols the Workbench draws, as inline SVG in `currentColor`. */
import type { CSSProperties } from "react";

type Props = { size?: number; style?: CSSProperties; className?: string };

function Svg({ size = 14, style, className, children }: Props & { children: React.ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" style={style} className={className} aria-hidden>
      {children}
    </svg>
  );
}

/** chart.xyaxis.line */
export const ChartLine = (p: Props) => <Svg {...p}><path d="M4 4v16h16" /><path d="M7 15l4-5 3 3 5-7" /></Svg>;
/** bubble.left.and.text.bubble.right */
export const Bubbles = (p: Props) => <Svg {...p}><path d="M3 5h9v7H7l-3 3z" /><path d="M13 9h8v7h-2l-3 3v-3h-3z" /><path d="M15.5 12h3M15.5 14.5h3" strokeWidth={1.4} /></Svg>;
/** clock.arrow.circlepath */
export const ClockArrow = (p: Props) => <Svg {...p}><path d="M20 12a8 8 0 1 1-2.5-5.8" /><path d="M20 4v5h-5" /><path d="M12 8v4l2.5 1.5" /></Svg>;
/** puzzlepiece.extension */
export const Puzzle = (p: Props) => <Svg {...p}><path d="M10 4a2 2 0 1 1 4 0v1h4v4h-1a2 2 0 1 0 0 4h1v4h-4v-1a2 2 0 1 0-4 0v1H6v-4h1a2 2 0 1 0 0-4H6V5h4z" /></Svg>;
/** gearshape */
export const Gear = (p: Props) => <Svg {...p}><circle cx="12" cy="12" r="3" /><path d="M19.4 15a1.7 1.7 0 0 0 .3 1.8l.1.1a2 2 0 1 1-2.8 2.8l-.1-.1a1.7 1.7 0 0 0-1.8-.3 1.7 1.7 0 0 0-1 1.5V21a2 2 0 1 1-4 0v-.1a1.7 1.7 0 0 0-1.1-1.5 1.7 1.7 0 0 0-1.8.3l-.1.1a2 2 0 1 1-2.8-2.8l.1-.1a1.7 1.7 0 0 0 .3-1.8 1.7 1.7 0 0 0-1.5-1H3a2 2 0 1 1 0-4h.1a1.7 1.7 0 0 0 1.5-1.1 1.7 1.7 0 0 0-.3-1.8l-.1-.1a2 2 0 1 1 2.8-2.8l.1.1a1.7 1.7 0 0 0 1.8.3H9a1.7 1.7 0 0 0 1-1.5V3a2 2 0 1 1 4 0v.1a1.7 1.7 0 0 0 1 1.5 1.7 1.7 0 0 0 1.8-.3l.1-.1a2 2 0 1 1 2.8 2.8l-.1.1a1.7 1.7 0 0 0-.3 1.8V9a1.7 1.7 0 0 0 1.5 1H21a2 2 0 1 1 0 4h-.1a1.7 1.7 0 0 0-1.5 1z" /></Svg>;
/** moon / sun.max */
export const Moon = (p: Props) => <Svg {...p}><path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z" /></Svg>;
export const Sun = (p: Props) => <Svg {...p}><circle cx="12" cy="12" r="4" /><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" /></Svg>;
/** arrow.clockwise */
export const Refresh = (p: Props) => <Svg {...p}><path d="M20 12a8 8 0 1 1-2.6-5.9" /><path d="M20 4v5h-5" /></Svg>;
/** magnifyingglass */
export const Search = (p: Props) => <Svg {...p}><circle cx="11" cy="11" r="6.5" /><path d="M20 20l-4.3-4.3" /></Svg>;
/** xmark.circle.fill */
export const XCircle = (p: Props) => <Svg {...p}><circle cx="12" cy="12" r="9" fill="currentColor" stroke="none" /><path d="M9 9l6 6M15 9l-6 6" stroke="var(--wb-on-fill, #fff)" /></Svg>;
/** chevron.up / down / left / right */
export const ChevronUp = (p: Props) => <Svg {...p}><path d="M6 15l6-6 6 6" /></Svg>;
export const ChevronDown = (p: Props) => <Svg {...p}><path d="M6 9l6 6 6-6" /></Svg>;
export const ChevronLeft = (p: Props) => <Svg {...p}><path d="M15 6l-6 6 6 6" /></Svg>;
export const ChevronRight = (p: Props) => <Svg {...p}><path d="M9 6l6 6-6 6" /></Svg>;
/** folder */
export const Folder = (p: Props) => <Svg {...p}><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></Svg>;
/** calendar */
export const Calendar = (p: Props) => <Svg {...p}><rect x="3" y="5" width="18" height="16" rx="2" /><path d="M3 10h18M8 3v4M16 3v4" /></Svg>;
/** slider.horizontal.3 */
export const Sliders = (p: Props) => <Svg {...p}><path d="M4 7h10M18 7h2M4 12h4M12 12h8M4 17h12M20 17h0" /><circle cx="15" cy="7" r="2" /><circle cx="9" cy="12" r="2" /><circle cx="17" cy="17" r="2" /></Svg>;
/** arrow.up.arrow.down */
export const Sort = (p: Props) => <Svg {...p}><path d="M8 4v16M8 4L5 7M8 4l3 3" /><path d="M16 20V4M16 20l3-3M16 20l-3-3" /></Svg>;
/** text.magnifyingglass */
export const TextSearch = (p: Props) => <Svg {...p}><path d="M3 6h10M3 10h7M3 14h5" /><circle cx="16" cy="14" r="4" /><path d="M19 17l3 3" /></Svg>;
/** building.2 (company) */
export const Building = (p: Props) => <Svg {...p}><path d="M3 21h18M5 21V4h8v17M13 9h6v12" /><path d="M8 8h2M8 12h2M8 16h2M16 13h1M16 17h1" strokeWidth={1.5} /></Svg>;
/** sum / dollarsign */
export const Sigma = (p: Props) => <Svg {...p}><path d="M18 5H6l6 7-6 7h12" /></Svg>;
export const Dollar = (p: Props) => <Svg {...p}><path d="M12 2v20" /><path d="M17 6.5c0-1.9-2.2-3-5-3s-5 1.1-5 3 2.2 3 5 3 5 1.1 5 3-2.2 3-5 3-5-1.1-5-3" /></Svg>;
/** chart.bar (auto granularity) */
export const ChartBar = (p: Props) => <Svg {...p}><path fill="currentColor" stroke="none" d="M4 13h4v7H4zM10 4h4v16h-4zM16 9h4v11h-4z" /></Svg>;
/** checkmark.circle.fill */
export const CheckCircle = (p: Props) => <Svg {...p}><circle cx="12" cy="12" r="10" fill="currentColor" stroke="none" /><path d="M8 12.5l2.8 2.8L16.5 9.5" stroke="var(--wb-on-fill, #fff)" strokeWidth={2.4} /></Svg>;
/** doc.on.doc (copy) */
export const Copy = (p: Props) => <Svg {...p}><rect x="9" y="9" width="11" height="11" rx="2" /><path d="M5 15V6a2 2 0 0 1 2-2h9" /></Svg>;
/** terminal */
export const Terminal = (p: Props) => <Svg {...p}><rect x="3" y="4" width="18" height="16" rx="2" /><path d="M7 9l3 3-3 3M12 15h5" /></Svg>;
/** list.bullet (outline) */
export const ListBullet = (p: Props) => <Svg {...p}><path d="M9 6h11M9 12h11M9 18h11" /><circle cx="5" cy="6" r="1" fill="currentColor" /><circle cx="5" cy="12" r="1" fill="currentColor" /><circle cx="5" cy="18" r="1" fill="currentColor" /></Svg>;
/** trash */
export const Trash = (p: Props) => <Svg {...p}><path d="M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3" /></Svg>;

// ── Settings sidebar symbols (the native SF Symbols, redrawn) ─────────────
const stroke = (size: number) => ({ width: size, height: size, viewBox: "0 0 16 16", fill: "none", stroke: "currentColor", strokeWidth: 1.4, strokeLinecap: "round" as const, strokeLinejoin: "round" as const });

export function Monitor({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <rect x="1.5" y="2.5" width="13" height="8.5" rx="1.5" />
      <path d="M5.5 13.5h5M8 11v2.5" />
    </svg>
  );
}
export function Nodes({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <circle cx="3.5" cy="12" r="1.7" />
      <circle cx="12.5" cy="12" r="1.7" />
      <circle cx="8" cy="4" r="1.7" />
      <path d="M5 11l2-5.5M11 11L9 5.5M5.2 12h5.6" />
    </svg>
  );
}
export function Antenna({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <circle cx="8" cy="7" r="1.5" />
      <path d="M8 8.5V14M5.2 9.8a4 4 0 0 1 0-5.6M10.8 4.2a4 4 0 0 1 0 5.6M3.4 11.6a6.5 6.5 0 0 1 0-9.2M12.6 2.4a6.5 6.5 0 0 1 0 9.2" />
    </svg>
  );
}
export function Hand({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <path d="M5 8.5V3.2a1 1 0 0 1 2 0V8M7 7.5V2.2a1 1 0 0 1 2 0V8M9 7.8V3.2a1 1 0 0 1 2 0v5.3M11 8.3V5.2a1 1 0 0 1 2 0V10c0 2.8-1.8 4.5-4.5 4.5S4 12.6 4 10.6V9.2a1 1 0 0 1 1.9-.4" />
    </svg>
  );
}
export function Windows({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <rect x="1.5" y="4.5" width="9.5" height="9" rx="1.5" />
      <path d="M5 4.5V3a1.5 1.5 0 0 1 1.5-1.5H13A1.5 1.5 0 0 1 14.5 3v6.5A1.5 1.5 0 0 1 13 11h-2" />
    </svg>
  );
}
export function Split({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <rect x="1.5" y="2.5" width="13" height="11" rx="2" />
      <path d="M7 2.5v11" />
    </svg>
  );
}
export function Cookie({ size = 14 }: { size?: number }) {
  return (
    <svg {...stroke(size)} aria-hidden="true">
      <path d="M8.5 1.6A6.4 6.4 0 1 0 14.4 8a2.4 2.4 0 0 1-2.8-2.2A2.4 2.4 0 0 1 9 3.4a2.2 2.2 0 0 1-.5-1.8z" />
      <path d="M5.5 6.5h.01M6 10.5h.01M9.5 11h.01M10.5 8h.01" strokeWidth="2" />
    </svg>
  );
}
