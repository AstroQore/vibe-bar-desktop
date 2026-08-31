import { useEffect, useState } from "react";

import type { PresentationSettings, QuotaBucket, QuotaView } from "../api";
import { api, formatCountdown, quotaBarColor } from "../api";
import { bucketLabelFor, subProviderFor } from "../naming";

const DEFAULT_FIELDS = ["codex.weekly", "claude.weekly", "claude.five_hour"];

export function MiniQuota() {
  const [view, setView] = useState<QuotaView | null>(null);
  const [settings, setSettings] = useState<PresentationSettings | null>(null);

  useEffect(() => {
    const refreshSettings = () => api.presentationSettings().then(setSettings).catch(() => undefined);
    api.quotaView().then(setView).catch(() => undefined);
    refreshSettings();
    const unlistenQuota = api.onQuotaUpdated((next) => {
      setView(next);
      refreshSettings();
    });
    const unlistenShown = api.onMiniShown(refreshSettings);
    return () => {
      unlistenQuota.then((off) => off()).catch(() => undefined);
      unlistenShown.then((off) => off()).catch(() => undefined);
    };
  }, []);

  const fields = settings?.selectedFieldIds.length
    ? settings.selectedFieldIds
    : [...new Set([
        ...DEFAULT_FIELDS,
        ...(view?.accounts.flatMap((account) =>
          account.buckets[0] ? [`${account.tool}.${account.buckets[0].id}`] : []) ?? []),
      ])];
  const rows = view ? fields.slice(0, 4).flatMap((field) => resolveField(view, settings, field)) : [];

  return (
    <main className="mini-quota" data-tauri-drag-region>
      <div className="mini-title" data-tauri-drag-region="deep">
        <span>Vibe Bar</span>
        <button
          className="mini-close"
          aria-label="Hide Mini"
          data-tauri-drag-region="false"
          onClick={() => void api.hideMini().catch(() => undefined)}
        >
          ×
        </button>
      </div>
      {!view ? (
        <p className="mini-empty">Loading quota…</p>
      ) : rows.length === 0 ? (
        <p className="mini-empty">No configured quota is available.</p>
      ) : (
        rows.map((row) => <MiniRow key={row.id} {...row} />)
      )}
    </main>
  );
}

function resolveField(view: QuotaView, settings: PresentationSettings | null, field: string) {
  const separator = field.indexOf(".");
  if (separator <= 0 || separator === field.length - 1) return [];
  const tool = field.slice(0, separator);
  const bucketId = field.slice(separator + 1);
  const bucket = view.accounts
    .filter((account) => account.tool === tool)
    .flatMap((account) => account.buckets)
    .find((candidate) => candidate.id === bucketId);
  if (!bucket) return [];
  const remaining = Math.max(0, 100 - bucket.usedPercent);
  const used = settings?.displayMode === "used";
  const bucketLabel = bucketLabelFor(tool, bucketId, bucket.title, bucket.shortLabel, bucket.groupTitle);
  return [{
    id: field,
    label: settings?.customLabels[field] || `${subProviderFor(tool, bucketId)} ${bucketLabel}`,
    bucket,
    value: used ? bucket.usedPercent : remaining,
    showsUsed: used,
    suffix: used ? "used" : "left",
  }];
}

function MiniRow({
  label,
  bucket,
  value,
  showsUsed,
  suffix,
}: {
  label: string;
  bucket: QuotaBucket;
  value: number;
  showsUsed: boolean;
  suffix: string;
}) {
  return (
    <section className="mini-row">
      <div className="mini-row-head">
        <span>{label}</span>
        {formatCountdown(bucket.resetAt) ? <small>{formatCountdown(bucket.resetAt)}</small> : null}
        <strong>{Math.round(value)}% {suffix}</strong>
      </div>
      <div className="track">
        <div
          className="fill"
          style={{ width: `${value}%`, background: quotaBarColor(value, showsUsed) }}
        />
      </div>
    </section>
  );
}
