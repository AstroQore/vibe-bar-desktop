/** Extra rows for `/preview.html?surface=settings`. */
import type { AppInfo, EffectiveModelPricingRow } from "../../api";

export const FIXTURE_INFO: AppInfo = {
  version: "0.1.0",
  dataRoot: "/Users/example/.vibebar",
  isDemo: true,
  nativeApp: { installed: true } as AppInfo["nativeApp"],
  onboarding: "skip",
};

export const FIXTURE_PRICING: EffectiveModelPricingRow[] = [
  { provider: "codex", company: "OpenAI", subProvider: "ChatGPT Agentic", model: "gpt-5.4", inputPerMillion: 1.25, outputPerMillion: 10, cacheReadPerMillion: 0.125, cacheWritePerMillion: null, fastMultiplier: 2 },
  { provider: "claude", company: "Anthropic", subProvider: "Claude", model: "claude-opus-4-1", inputPerMillion: 15, outputPerMillion: 75, cacheReadPerMillion: 1.5, cacheWritePerMillion: 18.75 },
  { provider: "claude", company: "Anthropic", subProvider: "Claude", model: "claude-sonnet-4-5", inputPerMillion: 3, outputPerMillion: 15, cacheReadPerMillion: 0.3, cacheWritePerMillion: 3.75, thresholdTokens: 200_000, inputAboveThresholdPerMillion: 6, outputAboveThresholdPerMillion: 22.5 },
  { provider: "gemini", company: "Google AI", subProvider: "Gemini Web", model: "gemini-2.5-pro", inputPerMillion: 1.25, outputPerMillion: 10, cacheReadPerMillion: 0.31, cacheWritePerMillion: null },
];
