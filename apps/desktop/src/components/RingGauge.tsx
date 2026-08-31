/**
 * The gauge the mini window is built from, matching the native `RingGauge`.
 *
 * An open arc rather than a full circle: the gap at the bottom is where the
 * eye expects a dial to start and end, so a nearly-full ring cannot be
 * mistaken for an empty one. The geometry is the native app's — 78% of the
 * circle, rotated so the opening sits at the bottom — because two clients
 * drawing the same number at different angles is the same problem as drawing
 * it in two colours.
 */

/** How much of the circle the dial spans. */
const ARC_FRACTION = 0.78;
/** Where the arc begins, so the missing fifth is centred at the bottom. */
const ROTATION_DEGREES = 90 + (1 - ARC_FRACTION) * 180;
/** Half-width of the pace marker, as a fraction of the whole circle. */
const MARKER_SPAN = 0.012;

interface Props {
  /** 0–100, in whichever direction the surface is showing. */
  percent: number;
  /** Where the same number is expected to land, drawn as a marker on the arc.
   *  Omitted when there is nothing to compare against. */
  expected?: number;
  color: string;
  markerColor?: string;
  size?: number;
  lineWidth?: number;
  /** Centre content — the percentage, normally. */
  children?: React.ReactNode;
}

export function RingGauge({
  percent,
  expected,
  color,
  markerColor,
  size = 48,
  lineWidth = 5,
  children,
}: Props) {
  const radius = (size - lineWidth) / 2;
  const circumference = 2 * Math.PI * radius;
  const filled = Math.min(100, Math.max(0, percent)) / 100;

  // A dash the length of the arc followed by a gap the length of everything
  // else draws exactly the arc, and an offset slides it to where it starts.
  const arc = (fraction: number) => ({
    strokeDasharray: `${fraction * circumference} ${circumference}`,
  });

  const marker =
    expected !== undefined && expected > 0 && expected < 100
      ? ARC_FRACTION * (Math.min(100, Math.max(0, expected)) / 100)
      : undefined;

  return (
    <span className="ring-gauge" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} aria-hidden>
        <g transform={`rotate(${ROTATION_DEGREES} ${size / 2} ${size / 2})`}>
          <circle
            className="ring-track"
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            strokeWidth={lineWidth}
            strokeLinecap="round"
            style={arc(ARC_FRACTION)}
          />
          <circle
            cx={size / 2}
            cy={size / 2}
            r={radius}
            fill="none"
            stroke={color}
            strokeWidth={lineWidth}
            strokeLinecap="round"
            style={arc(ARC_FRACTION * filled)}
          />
          {marker !== undefined && (
            <>
              {/* Twice: a wide translucent halo so the marker reads against a
                  fill of the same hue, then the mark itself. */}
              <circle
                cx={size / 2}
                cy={size / 2}
                r={radius}
                fill="none"
                stroke={markerColor ?? color}
                strokeOpacity={0.2}
                strokeWidth={lineWidth + 5}
                strokeLinecap="round"
                style={{
                  strokeDasharray: `${2 * MARKER_SPAN * circumference} ${circumference}`,
                  strokeDashoffset: `${-(marker - MARKER_SPAN) * circumference}`,
                }}
              />
              <circle
                cx={size / 2}
                cy={size / 2}
                r={radius}
                fill="none"
                stroke={markerColor ?? color}
                strokeWidth={lineWidth + 1}
                strokeLinecap="round"
                style={{
                  strokeDasharray: `${2 * MARKER_SPAN * circumference} ${circumference}`,
                  strokeDashoffset: `${-(marker - MARKER_SPAN) * circumference}`,
                }}
              />
            </>
          )}
        </g>
      </svg>
      {children !== undefined && <span className="ring-centre">{children}</span>}
    </span>
  );
}
