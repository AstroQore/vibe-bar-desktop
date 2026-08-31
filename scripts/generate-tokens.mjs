#!/usr/bin/env node
// Regenerates apps/desktop/src/tokens.ts from docs/contracts/design-tokens-v1.json.
//
// The contract is generated from the native Theme.swift and checked against it
// in that repository. Editing tokens.ts directly would make this client
// disagree with the other one while both test suites still pass, so the file
// carries a header saying not to and this script exists to make that easy.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const doc = JSON.parse(
  readFileSync(join(root, "docs/contracts/design-tokens-v1.json"), "utf8"),
);

const accents = Object.keys(doc.providerAccent)
  .sort()
  .map((tool) => {
    const value = doc.providerAccent[tool];
    return typeof value === "string"
      ? `  ${tool}: "${value}",`
      : `  ${tool}: { light: "${value.light}", dark: "${value.dark}" },`;
  })
  .join("\n");

const resetHistory = Object.keys(doc.resetHistoryAccent)
  .sort()
  .map((tool) => `  ${tool}: "${doc.resetHistoryAccent[tool]}",`)
  .join("\n");

const bar = doc.quotaBar;
const out = `// Generated from docs/contracts/design-tokens-v1.json — do not edit by hand.
// The contract is generated from the native Theme.swift and checked against it
// there, so an edit here would make this client disagree with the other one
// while both tests still pass.
//
// Regenerate with: pnpm run tokens

/** A provider that is teal in one client and green in the other is two
 *  providers as far as the reader is concerned. */
export const PROVIDER_ACCENT: Record<string, string | { light: string; dark: string }> = {
${accents}
};

/** The reset-history bars have their own palette — Claude is coral here and
 *  orange in PROVIDER_ACCENT. \`default\` is what an unlisted provider gets. */
export const RESET_HISTORY_ACCENT: Record<string, string> = {
${resetHistory}
};

export const QUOTA_BAR = {
  remaining: {
    criticalBelow: ${bar.remaining.criticalBelow},
    warningBelow: ${bar.remaining.warningBelow},
    critical: "${bar.remaining.critical}",
    warning: "${bar.remaining.warning}",
    ok: "${bar.remaining.ok}",
  },
  used: {
    criticalAtOrAbove: ${bar.used.criticalAtOrAbove},
    warningAtOrAbove: ${bar.used.warningAtOrAbove},
    critical: "${bar.used.critical}",
    warning: "${bar.used.warning}",
    ok: "${bar.used.ok}",
  },
  trackOpacity: ${bar.trackOpacity},
} as const;

/** The accent for one provider, resolved for the current appearance. */
export function providerAccent(tool: string, dark: boolean): string | undefined {
  const value = PROVIDER_ACCENT[tool];
  if (value === undefined) return undefined;
  return typeof value === "string" ? value : dark ? value.dark : value.light;
}
`;
writeFileSync(join(root, "apps/desktop/src/tokens.ts"), out);
console.log(
  `tokens.ts: ${Object.keys(doc.providerAccent).length} provider accents, ` +
    `${Object.keys(doc.resetHistoryAccent).length} reset-history accents`,
);
