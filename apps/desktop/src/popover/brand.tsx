/**
 * Native's `ToolBrandBadge` / `ToolBrandIconView`: the same SVG marks the
 * native app ships (`Resources/ProviderIcons/`), which this client carries
 * under `src/assets/providers/`. A badge is the icon centred in a fixed box
 * so titles line up whatever the mark's own proportions are.
 */
import { ProviderIcon } from "../components/ProviderIcon";

export function ToolBrandIcon({ tool, size, opacity }: { tool: string; size: number; opacity?: number }) {
  return (
    <span className="brand-icon" style={{ width: size, height: size, opacity }} aria-hidden>
      <ProviderIcon tool={tool} size={size} />
    </span>
  );
}

/** Native: `min(container, max(icon, container * 0.85))`. */
export function ToolBrandBadge({ tool, iconSize = 17, containerSize = 24 }: { tool: string; iconSize?: number; containerSize?: number }) {
  const effective = Math.min(containerSize, Math.max(iconSize, containerSize * 0.85));
  return (
    <span className="brand-badge" style={{ width: containerSize, height: containerSize }} aria-hidden>
      <ProviderIcon tool={tool} size={effective} />
    </span>
  );
}
