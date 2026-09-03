/**
 * What `invoke` answers when there is no Tauri underneath — `pnpm dev` in a
 * plain browser and headless screenshots of the whole app. Every command the
 * frontend calls has a synthetic answer here, built from the same fixtures
 * the preview surfaces use, so the App renders exactly as it does in the
 * app, with the app's CSS order, and can be looked at without a build.
 *
 * Nothing here reaches production paths: the router below only runs when
 * `__TAURI_INTERNALS__` is absent from the window.
 */
import { FIXTURE_COST, FIXTURE_NOW, FIXTURE_SETTINGS, FIXTURE_STATUS, FIXTURE_VIEW } from "../popover/fixture";
import { FIXTURE_USAGE } from "../workbench/usage/fixture";
import { FIXTURE_SESSIONS, FIXTURE_TRANSCRIPT } from "../workbench/sessions/fixture";
import { FIXTURE_RESET_HISTORY } from "../workbench/resets/fixture";
import { FIXTURE_SKILLS } from "../workbench/skills/fixture";
import { FIXTURE_INFO, FIXTURE_PRICING } from "../workbench/settings/fixture";

/** The fixtures' "now", for pages that take a clock. */
export const FIXTURE_NOW_SECONDS = FIXTURE_NOW;

export function inTauri(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const settings: Record<string, unknown> = {
  ...(FIXTURE_SETTINGS as unknown as Record<string, unknown>),
  selectedFieldIds: ["codex.weekly", "claude.weekly", "grok.weekly", "cursor.models"],
  customLabels: { "codex.weekly": "ChatGPT", "claude.weekly": "Claude", "grok.weekly": "Grok", "cursor.models": "Cursor" },
  menuBar: { isVisible: true, showTitle: false, layout: "twoRows" },
  miscProviderInstances: [
    { id: "copilot-1", tool: "copilot", name: "Copilot", isVisible: true },
    { id: "opencodego-1", tool: "openCodeGo", name: "OpenCode Go", isVisible: true },
  ],
};

let raw: Record<string, unknown> = {
  displayMode: settings.displayMode,
  menuBarColorBasis: settings.menuBarColorBasis,
  refreshIntervalSeconds: settings.refreshIntervalSeconds,
  updateChannel: settings.updateChannel,
  popoverDensity: settings.popoverDensity ?? "regular",
  menuBarItems: [
    {
      kind: "primary",
      isVisible: true,
      showTitle: false,
      layout: "twoRows",
      selectedFieldIds: settings.selectedFieldIds,
      customLabels: settings.customLabels,
      fieldStyles: { "codex.weekly": "logoAndPercent", "claude.weekly": "logoAndPercent" },
    },
  ],
  visibleCoreProviders: settings.visibleCoreProviders ?? ["codex", "claude", "gemini", "grok"],
  coreProviderOrder: settings.coreProviderOrder ?? ["codex", "claude", "gemini", "grok"],
  visibleMiscProviders: ["copilot-1"],
  miscProviderInstances: settings.miscProviderInstances,
  providerPlanLabels: {},
  costData: { privacyModeEnabled: false, retentionDays: 90 },
  miniWindow: { displayMode: "regular", windows: [{ stripDensity: "twoLine" }] },
  menuBarBlockAlertSuppressed: false,
  menuBarAutoRepairEnabled: false,
};

const health = {
  state: "healthy",
  message: "Control Center allow-list is clean",
  checkedAt: FIXTURE_NOW,
  needsFullDiskAccess: false,
  alertsEnabled: true,
  autoRepairEnabled: false,
  repairCommand: 'python3 "/Applications/Vibe Bar Desktop.app/Contents/Resources/resources/fix_menu_bar_allowlist.py" --bundle-id com.astroqore.VibeBarDesktop --apply',
};

let autostart = false;

export async function fixtureInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  const answer = (value: unknown) => value as T;
  switch (command) {
    case "quota_view":
    case "refresh_quota":
      return answer(FIXTURE_VIEW);
    case "presentation_settings":
      return answer(settings);
    case "shared_settings_raw":
      return answer(raw);
    case "save_shared_settings": {
      const changes = (args?.changes ?? {}) as Record<string, unknown>;
      raw = { ...raw, ...changes };
      for (const [key, value] of Object.entries(changes)) (settings as Record<string, unknown>)[key] = value;
      return answer(settings);
    }
    case "status_snapshot":
    case "refresh_status":
      return answer(FIXTURE_STATUS);
    case "cost_view":
    case "refresh_cost":
      return answer(FIXTURE_COST);
    case "usage_stats":
      return answer(FIXTURE_USAGE);
    case "session_list":
    case "session_search":
    case "session_listing":
      return answer(FIXTURE_SESSIONS);
    case "session_delete":
      return ((args as { sessionRefs: string[] }).sessionRefs.map((sessionRef) => ({ sessionRef, deleted: true }))) as T;
    case "session_transcript":
      return answer(FIXTURE_TRANSCRIPT);
    case "quota_cycles": {
      const key = `${String(args?.accountId)}:${String(args?.bucketId)}`;
      return answer({ completed: FIXTURE_RESET_HISTORY[key] ?? [], current: null });
    }
    case "app_info":
      return answer(FIXTURE_INFO);
    case "skills_set_projection":
      return true as T;
    case "skills_uninstall":
      return { backupPath: "/Users/example/.vibebar/skill_backups/20260903_101500_preview", removedByApp: {} } as T;
    case "skills_backups":
      return [] as T;
    case "skills_restore_backup":
    case "skills_install_local":
    case "skills_adopt":
      return undefined as T;
    case "skills_inventory":
      return answer(FIXTURE_SKILLS);
    case "pricing_effective":
      return answer(FIXTURE_PRICING);
    case "menu_bar_health":
    case "menu_bar_check_now":
    case "menu_bar_repair":
      return answer(health);
    case "autostart_enabled":
      return answer(autostart);
    case "set_autostart":
      autostart = Boolean(args?.enabled);
      return answer(autostart);
    case "pending_update":
      return null as T;
    case "complete_onboarding":
      return undefined as T;
    case "check_for_update":
      return answer(null);
    case "hide_mini":
    case "toggle_mini":
    case "resize_mini":
    case "hide_popover":
    case "resize_popover":
    case "show_main_window":
    case "install_update":
    case "open_in_terminal":
    case "reveal_path":
    case "open_url":
      return answer(undefined);
    default:
      throw new Error(`no fixture for ${command}`);
  }
}
