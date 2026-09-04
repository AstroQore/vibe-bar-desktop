/**
 * The Settings page's vocabulary and pure rules, after the native
 * `SettingsView` and `SettingsSidebarView`: the sidebar groups, the
 * section list, the pickers' options, and what this client can say about
 * each provider's connection routes.
 */
import type { PresentationSettings, QuotaView } from "../../api";
import { companyFor, subProviderFor } from "../../naming";
import { humanisedSettingName } from "../../settingNames";

export type SectionId =
  | "system"
  | "costData"
  | "pricing"
  | "mcp"
  | "remote"
  | "privacy"
  | "miniWindow"
  | "layout"
  | "openAI"
  | "anthropic"
  | "googleAI"
  | "xAI"
  | "browserCookies"
  | `misc:${string}`;

export interface SectionEntry {
  id: SectionId;
  title: string;
  group: "settings" | "core" | "misc";
  /** Brand icon id for provider rows; `null` for the symbol rows. */
  tool: string | null;
  /** The shared-settings visibility for provider rows, when known. */
  enabled?: boolean;
}

/** `SettingsSidebarView.basicPages`, in the native order, minus the two
 *  menu-bar pages: the menu bar is a macOS surface with no equivalent on
 *  Windows or Linux, and every setting on those pages belongs to the native
 *  app alone. This client neither reads nor writes them. */
export const BASIC_PAGES: ReadonlyArray<{ id: SectionId; title: string }> = [
  { id: "system", title: "System" },
  { id: "costData", title: "Cost Data" },
  { id: "pricing", title: "Model Pricing" },
  { id: "mcp", title: "MCP Server" },
  { id: "remote", title: "Remote Probes" },
  { id: "privacy", title: "Privacy" },
  { id: "miniWindow", title: "Mini Window" },
  { id: "layout", title: "Layout" },
];

export const CORE_PROVIDERS: ReadonlyArray<{ id: SectionId; title: string; tool: string }> = [
  { id: "openAI", title: "OpenAI", tool: "codex" },
  { id: "anthropic", title: "Anthropic", tool: "claude" },
  { id: "googleAI", title: "Google AI", tool: "gemini" },
  { id: "xAI", title: "SpaceXAI", tool: "grok" },
];

export function sections(settings: PresentationSettings | null): SectionEntry[] {
  const visibleCore = settings?.visibleCoreProviders ?? null;
  const out: SectionEntry[] = BASIC_PAGES.map((page) => ({ ...page, group: "settings", tool: null }));
  for (const provider of CORE_PROVIDERS) {
    out.push({ id: provider.id, title: provider.title, group: "core", tool: provider.tool, enabled: visibleCore === null || visibleCore.includes(provider.tool) });
  }
  out.push({ id: "browserCookies", title: "Browser Cookies", group: "misc", tool: null });
  const visibleMisc = settings?.visibleMiscProviders ?? null;
  for (const instance of settings?.miscProviderInstances ?? []) {
    const shown = instance.isVisible && (visibleMisc === null || visibleMisc.includes(instance.id));
    out.push({ id: `misc:${instance.id}`, title: instance.name || instance.tool, group: "misc", tool: instance.tool || null, enabled: shown });
  }
  return out;
}

export function filterSections(entries: SectionEntry[], search: string): SectionEntry[] {
  const needle = search.trim().toLowerCase();
  if (needle.length === 0) return entries;
  return entries.filter((entry) => entry.title.toLowerCase().includes(needle));
}

export const DENSITIES: ReadonlyArray<{ id: string; title: string; detail: string }> = [
  { id: "compact", title: "Compact", detail: "Tightest spacing, narrowest popover." },
  { id: "regular", title: "Regular", detail: "Balanced spacing — default." },
  { id: "spacious", title: "Spacious", detail: "Roomy spacing for big displays." },
];

export const REFRESH_OPTIONS = [60, 120, 300, 600, 900, 1800, 3600] as const;

/** `MiniWindowDisplayMode`, in the native order. */
export const MINI_LAYOUTS: ReadonlyArray<{ id: string; title: string }> = [
  { id: "regular", title: "Regular" },
  { id: "compact", title: "Compact" },
  { id: "ledger", title: "Ledger" },
  { id: "strip", title: "Strip" },
  { id: "tile", title: "Tile" },
  { id: "focus", title: "Focus" },
  { id: "rail", title: "Rail" },
];

export const STRIP_DENSITIES: ReadonlyArray<{ id: string; title: string }> = [
  { id: "roomy", title: "Roomy" },
  { id: "twoLine", title: "Two line" },
  { id: "narrow", title: "Narrow" },
];

/** `CostDataSettings.retentionOptions`: 0 is unlimited. */
export const RETENTION_OPTIONS: ReadonlyArray<{ days: number; title: string }> = [
  { days: 0, title: "Forever" },
  { days: 30, title: "30 days" },
  { days: 90, title: "90 days" },
  { days: 365, title: "1 year" },
  { days: 1095, title: "3 years" },
];

export const UPDATE_CHANNELS: ReadonlyArray<{ id: string; title: string }> = [
  { id: "main", title: "Stable" },
  { id: "dev", title: "Dev previews" },
];

export function replacedSummary(keys: string[]): string {
  const names = keys.map(humanisedSettingName);
  if (!names.length) return "";
  const listed = names.slice(0, 3).join(", ");
  if (names.length > 3) {
    return `${listed} and ${names.length - 3} more settings now hold the other copy's value.`;
  }
  return `${listed} now ${names.length === 1 ? "holds" : "hold"} the other copy's value.`;
}

export function formatInterval(seconds: number): string {
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}

export function providerList(tools: string[]): string {
  return tools.length ? tools.map((tool) => subProviderFor(tool)).join(", ") : "None";
}

/** The native `PrimaryProviderRoute.routes(for:)` names, per company. */
export const ROUTES: Record<string, ReadonlyArray<{ id: string; title: string; kind: "cli" | "oauth" | "cookies" | "file" | "probe" }>> = {
  openAI: [
    { id: "openAICLI", title: "CLI", kind: "cli" },
    { id: "openAIOAuth", title: "OAuth", kind: "oauth" },
    { id: "openAIBrowserCookies", title: "Chrome/Safari cookies", kind: "cookies" },
    { id: "openAIWebViewCookies", title: "WebView cookies", kind: "cookies" },
  ],
  anthropic: [
    { id: "claudeBrowserCookies", title: "Chrome/Safari cookies", kind: "cookies" },
    { id: "claudeWebViewCookies", title: "WebView cookies", kind: "cookies" },
    { id: "claudeOAuth", title: "OAuth", kind: "oauth" },
    { id: "claudeCLI", title: "CLI", kind: "cli" },
  ],
  googleAI: [
    { id: "geminiBrowserCookies", title: "Chrome/Safari cookies", kind: "cookies" },
    { id: "antigravityLocalProbe", title: "Local Antigravity / agy", kind: "probe" },
  ],
  xAI: [
    { id: "grokAuthJSON", title: "~/.grok/auth.json", kind: "file" },
    { id: "grokBrowserCookies", title: "Chrome/Safari cookies", kind: "cookies" },
  ],
};

export type RouteStatus = "found" | "missing" | "unused";

/** What this client can say about a route: CLI/OAuth/file routes are found
 *  when a quota was read for the company; cookie routes are not used here. */
export function routeStatus(kind: "cli" | "oauth" | "cookies" | "file" | "probe", section: SectionId, view: QuotaView | null): RouteStatus {
  if (kind === "cookies" || kind === "probe") return "unused";
  const company = CORE_PROVIDERS.find((p) => p.id === section);
  if (!company || !view) return "missing";
  const has = view.accounts.some((account) => companyFor(account.tool) === company.title && !account.error);
  return has ? "found" : "missing";
}

export function routeStatusTitle(status: RouteStatus): string {
  switch (status) {
    case "found":
      return "Found";
    case "missing":
      return "Missing";
    case "unused":
      return "Not used by this client";
  }
}

export function formatPerMillion(value: number | null | undefined): string {
  if (value == null) return "—";
  return value >= 100 ? `$${value.toFixed(0)}` : value >= 10 ? `$${value.toFixed(1)}` : `$${value.toFixed(2)}`;
}

export const READ_ONLY_NOTE = "Set in the native app; this client shows the shared value.";
