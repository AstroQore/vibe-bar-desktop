#!/usr/bin/env node
// Regenerates apps/desktop/src/naming.ts from docs/contracts/quota-naming-v1.json.
//
// The contract is generated from the native Swift sources and checked against
// them in that repository. This client used to keep its own copy of the
// tool-level half and had no access to the group-level half at all, so a
// bucket the native app files under "Spark" appeared here as its raw group
// title. Editing naming.ts directly would put that duplication back.
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const doc = JSON.parse(
  readFileSync(join(root, "docs/contracts/quota-naming-v1.json"), "utf8"),
);

const hierarchy = Object.keys(doc.hierarchy)
  .sort()
  .map((tool) => {
    const { company, subProvider } = doc.hierarchy[tool];
    return `  ${tool}: { company: ${JSON.stringify(company)}, subProvider: ${JSON.stringify(subProvider)} },`;
  })
  .join("\n");

const labels = Object.keys(doc.groupLabels)
  .sort()
  .map((key) => `  ${JSON.stringify(key)}: ${JSON.stringify(doc.groupLabels[key])},`)
  .join("\n");

const out = `// Generated from docs/contracts/quota-naming-v1.json — do not edit by hand.
// The contract is generated from the native Swift sources and checked against
// them there, so an edit here would make this client file a provider under a
// name the other one does not use.
//
// Regenerate with: pnpm run naming

/** L1 company and L2 SubProvider, by tool. */
export const HIERARCHY: Record<string, { company: string; subProvider: string }> = {
${hierarchy}
};

/** The short label an L3 group shows. A key absent here takes the bucket's
 *  own groupTitle, which is what the native app does for a group it has not
 *  been taught to shorten. */
export const GROUP_LABELS: Record<string, string> = {
${labels}
};

/** A bucket whose SubProvider is not its account's. Cursor reports Grok Bot. */
const SUB_PROVIDER_OVERRIDES: { tool: string; bucket: string; subProvider: string }[] =
${JSON.stringify(doc.subProviderOverrides, null, 2).replace(/\n/g, "\n  ")};

/** Buckets that sit directly under their SubProvider with no L3 group. */
const UNGROUPED: { tool: string; bucket: string }[] =
${JSON.stringify(doc.ungrouped.map(({ tool, bucket }) => ({ tool, bucket })), null, 2).replace(/\n/g, "\n  ")};

const EXACT: { tool?: string; bucket: string; key: string }[] =
${JSON.stringify(doc.groupKey.exact, null, 2).replace(/\n/g, "\n  ")};

const PATTERNS: { tool: string; contains?: string; anyOf?: string[]; key: string }[] =
${JSON.stringify(doc.groupKey.contains, null, 2).replace(/\n/g, "\n  ")};

const DEFAULT_GROUP: Record<string, string> = ${JSON.stringify(doc.groupKey.defaultGroup, null, 2).replace(/\n/g, "\n  ")};

const STEM_SUFFIXES: string[] = ${JSON.stringify(doc.groupKey.stemSuffixes)};

/** Which buckets are an L3 group of their own rather than part of their tool's
 *  default group. */
const BRANCH_STYLE: { alwaysTools: string[]; buckets: string[]; discoveredNeedGroupTitle: boolean } =
${JSON.stringify(doc.groupKey.branchStyle, null, 2).replace(/\n/g, "\n  ")};

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
 *  it carried a group title, which is why \`groupTitle\` has to be passed in. */
function isBranchStyle(tool: string, bucketId: string, groupTitle?: string): boolean {
  if (BRANCH_STYLE.alwaysTools.includes(tool)) return true;
  if (BRANCH_STYLE.buckets.includes(bucketId)) return true;
  return BRANCH_STYLE.discoveredNeedGroupTitle && Boolean(groupTitle);
}

/** The L3 group key, or null for a bucket that sits directly under its
 *  SubProvider. Follows the contract's stated order, which is load-bearing:
 *  Cursor first, then branch-style, then exact, then the patterns in order
 *  (\`flash-lite\` before \`flash\`), then the stem fallback. */
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
  return \`\${tool}.\${stem(bucketId)}\`;
}

/** How a bucket reads in a flat list: \`Spark Weekly\`, not
 *  \`GPT-5.3 Codex Spark Weekly\`.
 *
 *  The provider adapters already shorten this into \`shortLabel\`, and the
 *  native app prefers that, so this does too — the contract's group label is
 *  the fallback for a bucket whose adapter did not, and \`groupLabelFor\` is
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
  const short = shortLabel?.trim();
  if (short) return short;
  const group = groupLabelFor(tool, bucketId, groupTitle);
  return group && group !== bucketTitle
    ? \`\${group}\${separator}\${bucketTitle}\`
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
`;

writeFileSync(join(root, "apps/desktop/src/naming.ts"), out);
console.log(
  `naming.ts: ${Object.keys(doc.hierarchy).length} tools, ` +
    `${Object.keys(doc.groupLabels).length} group labels`,
);
