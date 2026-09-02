/**
 * A synthetic Usage Stats view for the preview page. Three harnesses, two
 * companies, a week of hourly-ish traffic with a quiet weekend, one unpriced
 * model, and one long model name — the shapes that broke layouts before.
 */
import type { GroupStat, TrendPoint, UsageStatsView } from "../../api";
import { FIXTURE_NOW } from "../../popover/fixture";

function point(bucketStart: number, scale: number): TrendPoint {
  const freshInput = Math.round(120_000 * scale);
  const output = Math.round(38_000 * scale);
  const cacheRead = Math.round(410_000 * scale);
  const cacheCreation = Math.round(60_000 * scale);
  return {
    bucketStart,
    requests: Math.round(48 * scale),
    freshInput,
    output,
    cacheRead,
    cacheCreation,
    totalTokens: freshInput + output + cacheRead + cacheCreation,
    costMicros: Math.round(3_950_000 * scale),
  };
}

const DAY = 86_400;
const end = FIXTURE_NOW;
const start = end - 7 * DAY;
const dayStart = (at: number) => {
  const date = new Date(at * 1000);
  date.setHours(0, 0, 0, 0);
  return date.getTime() / 1000;
};
const points: TrendPoint[] = [];
for (let at = dayStart(start); at < end; at += DAY) {
  const weekday = new Date(at * 1000).getDay();
  const scale = weekday === 0 || weekday === 6 ? 0.25 : 0.8 + 0.5 * Math.abs(Math.sin(at / DAY));
  points.push(point(at, scale));
}
const providers = [
  { harness: "Claude Code", company: "Anthropic", share: 0.62 },
  { harness: "Codex", company: "OpenAI", share: 0.3 },
  { harness: "Gemini Web", company: "Google AI", share: 0.08 },
].map(({ harness, company, share }) => ({
  harness,
  company,
  points: points.map((p) => ({
    ...p,
    requests: Math.round(p.requests * share),
    freshInput: Math.round(p.freshInput * share),
    output: Math.round(p.output * share),
    cacheRead: Math.round(p.cacheRead * share),
    cacheCreation: Math.round(p.cacheCreation * share),
    totalTokens: Math.round(p.totalTokens * share),
    costMicros: Math.round(p.costMicros * share),
  })),
}));

const sum = (key: keyof TrendPoint) => points.reduce((acc, p) => acc + (p[key] as number), 0);
const totalTokens = sum("totalTokens");

function group(name: string, company: string, share: number, unpriced = 0): GroupStat {
  return {
    name,
    company,
    requests: Math.round(sum("requests") * share),
    freshInput: Math.round(sum("freshInput") * share),
    output: Math.round(sum("output") * share),
    cacheRead: Math.round(sum("cacheRead") * share),
    cacheCreation: Math.round(sum("cacheCreation") * share),
    totalTokens: Math.round(totalTokens * share),
    costMicros: unpriced ? 0 : Math.round(sum("costMicros") * share),
    unpricedRequests: unpriced,
  };
}

export const FIXTURE_USAGE: UsageStatsView = {
  ledgerAvailable: true,
  privacySuppressed: false,
  scannedAt: end - 90,
  rangeStart: start,
  rangeEnd: end,
  summary: {
    requests: sum("requests"),
    freshInput: sum("freshInput"),
    output: sum("output"),
    cacheRead: sum("cacheRead"),
    cacheCreation: sum("cacheCreation"),
    totalTokens,
    costMicros: sum("costMicros"),
    unpricedRequests: 7,
    cacheHitRate: sum("cacheRead") / (sum("freshInput") + sum("cacheRead") + sum("cacheCreation")),
  },
  trend: { bucket: "day", points, providers },
  granularity: { hour: true, day: true, week: true },
  harnesses: [group("Claude Code", "Anthropic", 0.62), group("Codex", "OpenAI", 0.3), group("Gemini Web", "Google AI", 0.08)],
  providers: [group("Anthropic", "", 0.62), group("OpenAI", "", 0.3), group("Google AI", "", 0.08)],
  models: [
    group("claude-opus-4-1-20250805", "", 0.5),
    group("gpt-5-codex", "", 0.3),
    group("claude-sonnet-4-5", "", 0.12),
    group("gemini-2.5-pro-preview-with-a-very-long-suffix", "", 0.08, 7),
  ],
  requests: Array.from({ length: 24 }, (_, i) => {
    const harness = i % 3 === 0 ? "Codex" : i % 3 === 1 ? "Claude Code" : "Gemini Web";
    const freshInput = 1_200 + i * 37;
    const output = 400 + i * 11;
    const cacheRead = i % 4 === 0 ? 0 : 18_000 + i * 90;
    return {
      time: end - i * 731,
      harness,
      company: harness === "Codex" ? "OpenAI" : harness === "Claude Code" ? "Anthropic" : "Google AI",
      model: harness === "Codex" ? "gpt-5-codex" : harness === "Claude Code" ? "claude-opus-4-1-20250805" : "gemini-2.5-pro",
      tier: harness === "Claude Code" ? "standard" : null,
      freshInput,
      output,
      cacheRead,
      cacheCreation: i % 5 === 0 ? 3_000 : 0,
      totalTokens: freshInput + output + cacheRead + (i % 5 === 0 ? 3_000 : 0),
      costMicros: harness === "Gemini Web" ? null : 12_000 + i * 900,
      sessionId: `session-${i % 4}`,
    };
  }),
  totalRequests: 1_284,
  availableModels: ["claude-opus-4-1-20250805", "claude-sonnet-4-5", "gemini-2.5-pro-preview-with-a-very-long-suffix", "gpt-5-codex"],
  chipGroups: [
    { company: "OpenAI", harnesses: ["Codex"] },
    { company: "Anthropic", harnesses: ["Claude Code"] },
    { company: "Google AI", harnesses: ["Gemini Web"] },
  ],
};
