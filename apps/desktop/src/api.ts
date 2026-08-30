import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

/** Kept in sync with `vibebar_desktop_core::model`. */
export interface QuotaBucket {
  id: string;
  title: string;
  shortLabel: string;
  usedPercent: number;
  resetAt?: number;
  rawWindowSeconds?: number;
  groupTitle?: string;
}

export type QuotaOrigin = "live" | "desktopCache" | "sharedCache" | "mixed";

export interface QuotaErrorPayload {
  kind: string;
  detail?: string;
}

export interface AccountQuota {
  accountId: string;
  tool: string;
  buckets: QuotaBucket[];
  plan?: string;
  /** Unix epoch seconds. */
  queriedAt: number;
  origin: QuotaOrigin;
  error?: QuotaErrorPayload;
}

export interface QuotaView {
  accounts: AccountQuota[];
  lastUpdated?: number;
  hasSharedData: boolean;
  isDemo: boolean;
}

export type SessionSource = "indexed" | "scanned";

export interface SessionRow {
  rowId?: number;
  provider: string;
  harness: string;
  sessionId: string;
  title?: string;
  projectDir?: string;
  lastActiveAt?: number;
  /** Opaque backend-issued reference; never a filesystem path. */
  sessionRef: string;
  messageCount?: number;
  resumeCommand?: string;
  excerpt?: string;
}

export interface SessionListing {
  source: SessionSource;
  rows: SessionRow[];
  indexedTotal?: number;
  indexNote?: string;
}

export interface TranscriptMessage {
  role: "user" | "assistant" | "system" | "tool" | "other";
  text: string;
  timestamp?: string;
}

export interface TranscriptPage {
  messages: TranscriptMessage[];
  /** Omitted when a safety limit truncates a very large transcript scan. */
  totalMessages?: number;
  offset: number;
  truncated: boolean;
}

export interface NativeAppPresence {
  installed: boolean;
  running: boolean;
  bundleId: string;
}

export interface AppInfo {
  version: string;
  dataRoot: string;
  isDemo: boolean;
  nativeApp: NativeAppPresence;
}

export const QUOTA_EVENT = "vibebar://quota-updated";

export const api = {
  quotaView: () => invoke<QuotaView>("quota_view"),
  refreshQuota: () => invoke<QuotaView>("refresh_quota"),
  appInfo: () => invoke<AppInfo>("app_info"),
  sessionList: (limit = 100) => invoke<SessionListing>("session_list", { limit }),
  sessionSearch: (query: string, limit = 50) =>
    invoke<SessionListing>("session_search", { query, limit }),
  sessionTranscript: (
    sessionRef: string,
    offset = 0,
    limit = 50,
  ) =>
    invoke<TranscriptPage>("session_transcript", {
      sessionRef,
      offset,
      limit,
    }),
  onQuotaUpdated: (handler: (view: QuotaView) => void) =>
    listen<QuotaView>(QUOTA_EVENT, (event) => handler(event.payload)),
};

/** L1 company → L2 SubProvider naming, mirrored from the core crate so the
 *  UI groups providers exactly the way the native app does. */
const HIERARCHY: Record<string, { vendor: string; product: string }> = {
  codex: { vendor: "OpenAI", product: "ChatGPT Agentic" },
  claude: { vendor: "Anthropic", product: "Claude" },
  gemini: { vendor: "Google AI", product: "Gemini Web" },
  antigravity: { vendor: "Google AI", product: "AntiGravity" },
  grok: { vendor: "SpaceXAI", product: "Grok" },
  cursor: { vendor: "SpaceXAI", product: "Cursor" },
  copilot: { vendor: "GitHub", product: "Copilot" },
  alibaba: { vendor: "Alibaba", product: "Bailian" },
  alibabaTokenPlan: { vendor: "Alibaba", product: "Bailian" },
  zai: { vendor: "Zhipu", product: "GLM" },
  minimax: { vendor: "MiniMax", product: "MiniMax" },
  kimi: { vendor: "Moonshot", product: "Kimi" },
  mimo: { vendor: "Xiaomi", product: "MiMo" },
  iflytek: { vendor: "iFlytek", product: "Spark" },
  tencentHunyuan: { vendor: "Tencent", product: "Hunyuan" },
  tencentTokenPlan: { vendor: "Tencent", product: "Hunyuan" },
  volcengine: { vendor: "ByteDance", product: "Doubao" },
  volcengineAgentPlan: { vendor: "ByteDance", product: "Doubao" },
  baiduQianfan: { vendor: "Baidu", product: "Qianfan" },
  openCodeGo: { vendor: "OpenCode", product: "OpenCode Go" },
  kilo: { vendor: "Kilo", product: "Kilo" },
  kiro: { vendor: "Kiro", product: "Kiro" },
  ollama: { vendor: "Ollama", product: "Ollama" },
  openRouter: { vendor: "OpenRouter", product: "OpenRouter" },
  warp: { vendor: "Warp", product: "Warp" },
};

export function hierarchyFor(tool: string) {
  return HIERARCHY[tool] ?? { vendor: tool, product: tool };
}

export function severityFor(remainingPercent: number): "ok" | "warning" | "critical" {
  if (remainingPercent < 10) return "critical";
  if (remainingPercent < 30) return "warning";
  return "ok";
}

export function formatRelative(unixSeconds?: number): string {
  if (!unixSeconds) return "never";
  const deltaSeconds = Math.round(Date.now() / 1000 - unixSeconds);
  if (deltaSeconds < 60) return "just now";
  if (deltaSeconds < 3600) return `${Math.floor(deltaSeconds / 60)}m ago`;
  if (deltaSeconds < 86400) return `${Math.floor(deltaSeconds / 3600)}h ago`;
  return `${Math.floor(deltaSeconds / 86400)}d ago`;
}

/** "resets in 3h 12m", or empty when the bucket states no reset. */
export function formatCountdown(resetAt?: number): string {
  if (!resetAt) return "";
  const remaining = Math.round(resetAt - Date.now() / 1000);
  if (remaining <= 0) return "resetting";
  const hours = Math.floor(remaining / 3600);
  const minutes = Math.floor((remaining % 3600) / 60);
  if (hours >= 24) {
    const days = Math.floor(hours / 24);
    return `resets in ${days}d ${hours % 24}h`;
  }
  if (hours > 0) return `resets in ${hours}h ${minutes}m`;
  return `resets in ${minutes}m`;
}

const API_KEY_ENV: Record<string, string> = {
  zai: "Z_AI_API_KEY",
  minimax: "MINIMAX_CODING_API_KEY or MINIMAX_API_KEY",
  openRouter: "OPENROUTER_API_KEY",
  warp: "WARP_API_KEY or WARP_TOKEN",
};

export function describeError(error: QuotaErrorPayload, tool?: string): string {
  const apiKey = tool ? API_KEY_ENV[tool] : undefined;
  switch (error.kind) {
    case "noCredential":
      if (apiKey) return `Set ${apiKey} and refresh.`;
      return "Not signed in — run the provider's CLI login.";
    case "needsLogin":
      if (apiKey) return `The configured ${apiKey} was rejected. Update it and refresh.`;
      return "Credential rejected — sign in again with the provider's CLI.";
    case "rateLimited":
      return "Rate limited by the provider. Try again shortly.";
    case "network":
      return `Network error${error.detail ? `: ${error.detail}` : ""}`;
    case "timedOut":
      return "The provider did not respond in time.";
    case "notImplemented":
      return "No adapter in this build yet.";
    case "parseFailure":
      return `Unexpected response${error.detail ? `: ${error.detail}` : ""}`;
    default:
      return error.detail ?? "Unknown error";
  }
}
