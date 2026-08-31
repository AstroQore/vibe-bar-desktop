// Generated from docs/contracts/design-tokens-v1.json — do not edit by hand.
// The contract is generated from the native Theme.swift and checked against it
// there, so an edit here would make this client disagree with the other one
// while both tests still pass.
//
// Regenerate with: pnpm run tokens

/** A provider that is teal in one client and green in the other is two
 *  providers as far as the reader is concerned. */
export const PROVIDER_ACCENT: Record<string, string | { light: string; dark: string }> = {
  alibaba: "#FF9E33",
  alibabaTokenPlan: "#FF7A2E",
  antigravity: "#8C66EB",
  baiduQianfan: "#2966ED",
  claude: "#ED6666",
  codex: "#4DC7BD",
  copilot: "#757575",
  cursor: "#8C8CF5",
  gemini: "#579EF5",
  grok: { light: "#4D6178", dark: "#ADBFD4" },
  iflytek: "#1A5EBF",
  kilo: "#785EED",
  kimi: "#333333",
  kiro: "#3D7AF0",
  mimo: "#F78033",
  minimax: "#F74D73",
  ollama: "#2E2E2E",
  openCodeGo: "#38A880",
  openRouter: "#FA9E29",
  tencentHunyuan: "#007DE8",
  tencentTokenPlan: "#2E9EF5",
  volcengine: "#EB4D4D",
  volcengineAgentPlan: "#F57338",
  warp: "#948CB5",
  zai: "#42BD8C",
};

export const QUOTA_BAR = {
  remaining: {
    criticalBelow: 10,
    warningBelow: 30,
    critical: "#F54D4D",
    warning: "#F79E33",
    ok: "#2EBD8C",
  },
  used: {
    criticalAtOrAbove: 90,
    warningAtOrAbove: 70,
    critical: "#F54D4D",
    warning: "#F79E33",
    ok: "#33A8C7",
  },
  trackOpacity: 0.08,
} as const;

/** The accent for one provider, resolved for the current appearance. */
export function providerAccent(tool: string, dark: boolean): string | undefined {
  const value = PROVIDER_ACCENT[tool];
  if (value === undefined) return undefined;
  return typeof value === "string" ? value : dark ? value.dark : value.light;
}
