/**
 * The Skills page's pure logic, after the native `SkillsManagerPage` and
 * `SkillListRow`: the managed harness targets, each skill's activation state
 * per harness as far as this client can see it, the per-app counts, and the
 * filter.
 */
import type { SkillInventoryRow } from "../../api";

export type SkillApp = "codex" | "claude" | "gemini" | "antigravity" | "grok" | "cursor";

export interface SkillAppTarget {
  id: SkillApp;
  displayName: string;
  /** The harness has a per-skill off switch in its own configuration. */
  supportsNativeSwitch: boolean;
  /** The harness scans the shared root itself, so no link is needed. */
  discoversSharedRoot: boolean;
  /** Where a projection lands, relative to the home directory. */
  skillsPath: string;
  /** The config file that holds the native switch, when there is one. */
  configPath: string | null;
  switchKey: string | null;
}

/** `SkillAppTarget.managedHarnesses`, in the native order. */
export const MANAGED_APPS: readonly SkillAppTarget[] = [
  { id: "codex", displayName: "Codex", supportsNativeSwitch: true, discoversSharedRoot: true, skillsPath: ".codex/skills", configPath: ".codex/config.toml", switchKey: "[[skills.config]]" },
  { id: "claude", displayName: "Claude Code", supportsNativeSwitch: true, discoversSharedRoot: false, skillsPath: ".claude/skills", configPath: ".claude/settings.json", switchKey: "skillOverrides" },
  { id: "gemini", displayName: "Gemini CLI", supportsNativeSwitch: true, discoversSharedRoot: true, skillsPath: ".gemini/skills", configPath: ".gemini/settings.json", switchKey: "skills.disabled" },
  { id: "antigravity", displayName: "AntiGravity", supportsNativeSwitch: false, discoversSharedRoot: false, skillsPath: ".gemini/config/skills", configPath: null, switchKey: null },
  { id: "grok", displayName: "Grok Build", supportsNativeSwitch: true, discoversSharedRoot: true, skillsPath: ".grok/skills", configPath: ".grok/config.toml", switchKey: "[skills] disabled" },
  { id: "cursor", displayName: "Cursor", supportsNativeSwitch: false, discoversSharedRoot: true, skillsPath: ".cursor/skills", configPath: null, switchKey: null },
];

export const SSOT_PATH = ".agents/skills";

/** The native `SkillActivationState`, as far as the inventory can tell:
 *  this client sees links, not the harness's own switches. */
export type ActivationState = "enabled" | "coupled" | "notProjected" | "unknown";

export function activationState(row: SkillInventoryRow, app: SkillAppTarget): ActivationState {
  if (row.targets.includes(app.id)) return app.supportsNativeSwitch ? "unknown" : "enabled";
  if (app.discoversSharedRoot) return "coupled";
  return "notProjected";
}

export function stateTitle(state: ActivationState): string {
  switch (state) {
    case "notProjected":
      return "Not projected";
    case "enabled":
      return "Enabled";
    case "coupled":
      return "Available through a shared or compatibility root";
    case "unknown":
      return "Projected, native state unknown";
  }
}

export function isOn(state: ActivationState): boolean {
  return state === "enabled" || state === "coupled" || state === "unknown";
}

/** The native help text per circle, minus the click affordances this client
 *  does not offer. */
export function helpText(app: SkillAppTarget, state: ActivationState): string {
  switch (state) {
    case "notProjected":
      return `${app.displayName} — not projected.`;
    case "enabled":
      return `${app.displayName} — projected and enabled.`;
    case "coupled":
      return app.id === "cursor"
        ? `${app.displayName} — reads the shared skills root directly; there is nothing to toggle.`
        : `${app.displayName} — reads the shared skills root directly.`;
    case "unknown":
      return `${app.displayName} — projected; its own per-skill switch is not read by this client.`;
  }
}

/** Skills each harness sees: projected ones plus, for harnesses that scan
 *  the shared root, every healthy skill in it. */
export function appCounts(rows: SkillInventoryRow[]): Record<SkillApp, number> {
  const counts = Object.fromEntries(MANAGED_APPS.map((app) => [app.id, 0])) as Record<SkillApp, number>;
  for (const row of rows) {
    for (const app of MANAGED_APPS) {
      if (isOn(activationState(row, app))) counts[app.id] += 1;
    }
  }
  return counts;
}

export function appCountHelp(app: SkillAppTarget, rows: SkillInventoryRow[]): string {
  const count = appCounts(rows)[app.id];
  let help = `${app.displayName} sees ${count} skill${count === 1 ? "" : "s"}`;
  const linked = rows.filter((row) => row.targets.includes(app.id)).length;
  if (app.discoversSharedRoot) {
    const coupled = count - linked;
    help += ` · ${linked} linked + ${coupled} via ${app.id === "antigravity" ? "the Gemini CLI compatibility root" : "the shared skills root"}`;
  } else {
    help += ` · ${linked} linked`;
  }
  return help;
}

export function filterSkills(rows: SkillInventoryRow[], search: string): SkillInventoryRow[] {
  const needle = search.trim().toLowerCase();
  const sorted = [...rows].sort((a, b) => a.name.localeCompare(b.name));
  if (needle.length === 0) return sorted;
  return sorted.filter((row) => row.name.toLowerCase().includes(needle) || (row.description ?? "").toLowerCase().includes(needle));
}

export function countSummary(shown: number, total: number): string {
  if (shown === total) return `${total} skill${total === 1 ? "" : "s"}`;
  return `${shown} of ${total} skills`;
}

/** The source badge: a repository slug when the skill came from one, else `local`. */
export function sourceBadge(row: SkillInventoryRow): string {
  return row.source && row.source !== "local" ? row.source : "local";
}

/** Health worth a badge: anything the scan could not read as a skill. */
export function healthBadge(row: SkillInventoryRow): string | null {
  switch (row.health) {
    case "healthy":
      return null;
    case "symlink_ignored":
      return "SYMLINK";
    default:
      return "UNREADABLE";
  }
}

/** The wiring explanation, the native `SkillWiringView` text. */
export const WIRING = {
  title: "How skill syncing works",
  sourceOfTruth: `Every skill lives in ~/${SSOT_PATH}/<name>. Install, update, and uninstall all happen there and only there.`,
  projections:
    "Claude Code and AntiGravity read only their own skills folders, so Vibe Bar links (or copies) skills into them. Codex, Gemini CLI, Grok Build, and Cursor scan the shared root themselves — no link needed.",
  nativeSwitches:
    "Where a harness has its own per-skill off switch, the circles flip that switch. Cursor has none, so every skill in the shared root is always available to it — that is the “shared root” badge, and there is nothing to toggle.",
  footer: `Vibe Bar writes only inside ~/${SSOT_PATH}, the per-harness skills folders, and the config files above.`,
};

export function wiringLine(app: SkillAppTarget): string {
  if (app.discoversSharedRoot && app.supportsNativeSwitch) return `scans ~/${SSOT_PATH} directly · per-skill switch ${app.switchKey} in ~/${app.configPath}`;
  if (app.discoversSharedRoot) return `scans ~/${SSOT_PATH} directly · no per-skill switch`;
  if (app.supportsNativeSwitch) return `reads only ~/${app.skillsPath} · per-skill switch ${app.switchKey} in ~/${app.configPath}`;
  return `reads only ~/${app.skillsPath} · no per-skill switch; also reads ~/.gemini/skills`;
}
