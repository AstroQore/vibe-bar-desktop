import { providerAccent } from "../tokens";

// The brand marks are the native app's, byte for byte, so the same provider
// is recognisably the same in both clients. They are single-path monochrome
// SVGs, so `currentColor` lets one file serve both appearances.
const ICONS = import.meta.glob("../assets/providers/ProviderIcon-*.svg", {
  eager: true,
  query: "?raw",
  import: "default",
}) as Record<string, string>;

/** Icon filenames are lowercase; tool ids are not (`tencentHunyuan`). */
const BY_TOOL = new Map<string, string>(
  Object.entries(ICONS).map(([path, markup]) => {
    const name = path.split("/").pop() ?? "";
    const stem = name.replace(/^ProviderIcon-/, "").replace(/\.svg$/, "");
    return [stem.toLowerCase(), markup];
  }),
);

/** Tools whose mark is another tool's, because they are the same brand. */
const ALIASES: Record<string, string> = {
  alibabatokenplan: "alibaba",
  tencenttokenplan: "tencenthunyuan",
  volcengineagentplan: "volcengine",
  chatgptwork: "codex",
  grokbot: "grok",
  claudecowork: "claude",
};

function markupFor(tool: string): string | undefined {
  const key = tool.toLowerCase();
  return BY_TOOL.get(key) ?? BY_TOOL.get(ALIASES[key] ?? "");
}

/**
 * A provider's brand mark, tinted with its accent.
 *
 * Renders nothing when the tool has no mark, rather than a placeholder: a
 * generic glyph beside a real one reads as a provider whose brand failed to
 * load, which is worse than no glyph at all.
 */
export function ProviderIcon({
  tool,
  size = 16,
  dark = true,
}: {
  tool: string;
  size?: number;
  dark?: boolean;
}) {
  const markup = markupFor(tool);
  if (!markup) return null;
  // The source marks are `fill="white"`; recolouring through currentColor
  // keeps one file usable in both appearances.
  const tinted = markup
    .replace(/fill="white"/g, 'fill="currentColor"')
    .replace(/<svg /, `<svg width="${size}" height="${size}" `);
  return (
    <span
      className="provider-icon"
      style={{ color: providerAccent(tool, dark), width: size, height: size }}
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: tinted }}
    />
  );
}
