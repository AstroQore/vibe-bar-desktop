// Generated from docs/contracts/quota-naming-v1.json — do not edit by hand.
// The contract is generated from the native Swift sources and checked against
// them there, so an edit here would make this client file a provider under a
// name the other one does not use.
//
// Regenerate with: pnpm run naming

/** L1 company and L2 SubProvider, by tool. */
export const HIERARCHY: Record<string, { company: string; subProvider: string }> = {
  alibaba: { company: "Alibaba", subProvider: "Bailian" },
  alibabaTokenPlan: { company: "Alibaba", subProvider: "Bailian" },
  antigravity: { company: "Google AI", subProvider: "AntiGravity" },
  baiduQianfan: { company: "Baidu", subProvider: "Qianfan" },
  claude: { company: "Anthropic", subProvider: "Claude" },
  codex: { company: "OpenAI", subProvider: "ChatGPT Agentic" },
  copilot: { company: "GitHub", subProvider: "Copilot" },
  cursor: { company: "SpaceXAI", subProvider: "Cursor" },
  gemini: { company: "Google AI", subProvider: "Gemini Web" },
  grok: { company: "SpaceXAI", subProvider: "Grok" },
  iflytek: { company: "iFlytek", subProvider: "Spark" },
  kilo: { company: "Kilo", subProvider: "Kilo" },
  kimi: { company: "Moonshot", subProvider: "Kimi" },
  kiro: { company: "Kiro", subProvider: "Kiro" },
  mimo: { company: "Xiaomi", subProvider: "MiMo" },
  minimax: { company: "MiniMax", subProvider: "MiniMax" },
  ollama: { company: "Ollama", subProvider: "Ollama" },
  openCodeGo: { company: "OpenCode", subProvider: "OpenCode Go" },
  openRouter: { company: "OpenRouter", subProvider: "OpenRouter" },
  tencentHunyuan: { company: "Tencent", subProvider: "Hunyuan" },
  tencentTokenPlan: { company: "Tencent", subProvider: "Hunyuan" },
  volcengine: { company: "ByteDance", subProvider: "Doubao" },
  volcengineAgentPlan: { company: "ByteDance", subProvider: "Doubao" },
  warp: { company: "Warp", subProvider: "Warp" },
  zai: { company: "Zhipu", subProvider: "GLM" },
};

/** The short label an L3 group shows. A key absent here takes the bucket's
 *  own groupTitle, which is what the native app does for a group it has not
 *  been taught to shorten. */
export const GROUP_LABELS: Record<string, string> = {
  "antigravity.claude-gpt-models": "Claude + GPT",
  "antigravity.gemini-models": "Gemini",
  "claude.all-models": "All",
  "claude.design": "Design",
  "claude.fable": "Fable",
  "claude.oauth": "OAuth",
  "claude.opus": "Opus",
  "claude.routine": "Routine",
  "claude.sonnet": "Sonnet",
  "codex.all-models": "All",
  "codex.spark": "Spark",
  "cursor.models": "Cursor",
  "cursor.other-models": "Other",
  "gemini.all-models": "All",
  "gemini.flash": "Flash",
  "gemini.flash-lite": "Flash Lite",
  "gemini.pro": "Pro",
  "grok.all-models": "All",
};

/** A bucket whose SubProvider is not its account's. Cursor reports Grok Bot. */
const SUB_PROVIDER_OVERRIDES: { tool: string; bucket: string; subProvider: string }[] =
[
    {
      "bucket": "grok_bot_weekly",
      "subProvider": "Grok Bot",
      "tool": "cursor"
    }
  ];

/** Buckets that sit directly under their SubProvider with no L3 group. */
const UNGROUPED: { tool: string; bucket: string }[] =
[
    {
      "tool": "cursor",
      "bucket": "grok_bot_weekly"
    }
  ];

const EXACT: { tool?: string; bucket: string; key: string }[] =
[
    {
      "bucket": "gpt_5_3_codex_spark_five_hour",
      "key": "codex.spark"
    },
    {
      "bucket": "gpt_5_3_codex_spark_weekly",
      "key": "codex.spark"
    },
    {
      "bucket": "weekly_sonnet",
      "key": "claude.sonnet"
    },
    {
      "bucket": "weekly_design",
      "key": "claude.design"
    },
    {
      "bucket": "daily_routines",
      "key": "claude.routine"
    },
    {
      "bucket": "weekly_opus",
      "key": "claude.opus"
    },
    {
      "bucket": "weekly_fable",
      "key": "claude.fable"
    },
    {
      "bucket": "weekly_oauth_apps",
      "key": "claude.oauth"
    },
    {
      "bucket": "models",
      "key": "cursor.models",
      "tool": "cursor"
    },
    {
      "bucket": "other_models",
      "key": "cursor.other-models",
      "tool": "cursor"
    }
  ];

const PATTERNS: { tool: string; contains?: string; anyOf?: string[]; key: string }[] =
[
    {
      "contains": "flash-lite",
      "key": "gemini.flash-lite",
      "tool": "gemini"
    },
    {
      "contains": "flash",
      "key": "gemini.flash",
      "tool": "gemini"
    },
    {
      "contains": "pro",
      "key": "gemini.pro",
      "tool": "gemini"
    },
    {
      "anyOf": [
        "gemini_five_hour",
        "gemini_weekly"
      ],
      "key": "antigravity.gemini-models",
      "tool": "antigravity"
    },
    {
      "anyOf": [
        "claude_gpt_five_hour",
        "claude_gpt_weekly"
      ],
      "key": "antigravity.claude-gpt-models",
      "tool": "antigravity"
    },
    {
      "contains": "gpt-oss",
      "key": "antigravity.gpt-oss",
      "tool": "antigravity"
    },
    {
      "contains": "claude",
      "key": "antigravity.claude",
      "tool": "antigravity"
    },
    {
      "contains": "flash-lite",
      "key": "antigravity.gemini-flash-lite",
      "tool": "antigravity"
    },
    {
      "contains": "flash",
      "key": "antigravity.gemini-flash",
      "tool": "antigravity"
    },
    {
      "contains": "pro",
      "key": "antigravity.gemini-pro",
      "tool": "antigravity"
    }
  ];

const DEFAULT_GROUP: Record<string, string> = {
    "claude": "claude.all-models",
    "codex": "codex.all-models",
    "gemini": "gemini.all-models",
    "grok": "grok.all-models"
  };

const STEM_SUFFIXES: string[] = ["_five_hour","_weekly","_monthly","_daily","_primary","_secondary"];

/** Which buckets are an L3 group of their own rather than part of their tool's
 *  default group. */
const BRANCH_STYLE: { alwaysTools: string[]; buckets: string[]; discoveredNeedGroupTitle: boolean } =
{
    "alwaysTools": [
      "antigravity"
    ],
    "buckets": [
      "gpt_5_3_codex_spark_five_hour",
      "gpt_5_3_codex_spark_weekly",
      "weekly_sonnet",
      "weekly_design",
      "daily_routines",
      "weekly_opus",
      "weekly_fable",
      "weekly_oauth_apps"
    ],
    "discoveredNeedGroupTitle": true
  };

export function companyFor(tool: string): string {
  return HIERARCHY[tool]?.company ?? tool;
}

/** L2. One bucket belongs to a SubProvider its account does not. */
export function subProviderFor(tool: string, bucketId?: string): string {
  const override = SUB_PROVIDER_OVERRIDES.find(
    (rule) => rule.tool === tool && rule.bucket === bucketId,
  );
  return override?.subProvider ?? HIERARCHY[tool]?.subProvider ?? tool;
}

/** The bucket id with one window suffix removed, so both windows of a runtime
 *  quota group land in the same L3 group. */
function stem(bucketId: string): string {
  for (const suffix of STEM_SUFFIXES) {
    if (bucketId.endsWith(suffix)) {
      const trimmed = bucketId.slice(0, -suffix.length);
      return trimmed || bucketId;
    }
  }
  return bucketId;
}

/** Is this bucket an L3 group of its own, or does it belong to its tool's
 *  default group? A bucket discovered at runtime is its own group exactly when
 *  it carried a group title, which is why `groupTitle` has to be passed in. */
function isBranchStyle(tool: string, bucketId: string, groupTitle?: string): boolean {
  if (BRANCH_STYLE.alwaysTools.includes(tool)) return true;
  if (BRANCH_STYLE.buckets.includes(bucketId)) return true;
  return BRANCH_STYLE.discoveredNeedGroupTitle && Boolean(groupTitle);
}

/** The L3 group key, or null for a bucket that sits directly under its
 *  SubProvider. Follows the contract's stated order, which is load-bearing:
 *  Cursor first, then branch-style, then exact, then the patterns in order
 *  (`flash-lite` before `flash`), then the stem fallback. */
export function groupKeyFor(
  tool: string,
  bucketId: string,
  groupTitle?: string,
): string | null {
  if (UNGROUPED.some((rule) => rule.tool === tool && rule.bucket === bucketId)) {
    return null;
  }
  if (!isBranchStyle(tool, bucketId, groupTitle)) {
    return DEFAULT_GROUP[tool] ?? null;
  }

  const exact = EXACT.find(
    (rule) => rule.bucket === bucketId && (rule.tool === undefined || rule.tool === tool),
  );
  if (exact) return exact.key;

  const id = bucketId.toLowerCase();
  for (const rule of PATTERNS) {
    if (rule.tool !== tool) continue;
    if (rule.anyOf?.includes(id)) return rule.key;
    if (rule.contains && id.includes(rule.contains)) return rule.key;
  }
  return `${tool}.${stem(bucketId)}`;
}

/** How a bucket reads in a flat list: `Spark Weekly`, not
 *  `GPT-5.3 Codex Spark Weekly`.
 *
 *  The provider adapters already shorten this into `shortLabel`, and the
 *  native app prefers that, so this does too — the contract's group label is
 *  the fallback for a bucket whose adapter did not, and `groupLabelFor` is
 *  what a grouped surface uses for its column headers. A bucket with no group
 *  of its own is named by its window alone. */
export function bucketLabelFor(
  tool: string,
  bucketId: string,
  bucketTitle: string,
  shortLabel: string | undefined,
  groupTitle: string | undefined,
  separator = " ",
): string {
  // A bucket whose group *is* its SubProvider is drawn under a heading that
  // already says so, and naming the window is the only thing left to add.
  // Compared on the group title, as the native app does: Cursor's own models
  // sit in "Cursor Models", which is a group of its own and keeps its name.
  const reported = groupTitle?.trim();
  if (reported && reported.toLowerCase() === subProviderFor(tool, bucketId).toLowerCase()) {
    return bucketTitle;
  }
  const short = shortLabel?.trim();
  if (short) return short;
  const group = groupLabelFor(tool, bucketId, groupTitle);
  return group && group !== bucketTitle
    ? `${group}${separator}${bucketTitle}`
    : bucketTitle;
}

/** What an L3 group column is called. Falls back to the bucket's own
 *  groupTitle, which is what the native app shows for a group it has not been
 *  taught to shorten. Null means the bucket sits directly under its
 *  SubProvider with no group header of its own. */
export function groupLabelFor(
  tool: string,
  bucketId: string,
  groupTitle?: string,
): string | null {
  const key = groupKeyFor(tool, bucketId, groupTitle);
  if (key === null) return null;
  return GROUP_LABELS[key] ?? groupTitle ?? null;
}
