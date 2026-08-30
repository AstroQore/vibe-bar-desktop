import { useEffect, useState } from "react";

import type { PresentationSettings, QuotaBucket, QuotaView } from "../api";
import { api, formatCountdown, hierarchyFor, severityFor } from "../api";

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

  const fields = settings?.selectedFieldIds.length ? settings.selectedFieldIds : DEFAULT_FIELDS;
  const rows = view ? fields.slice(0, 4).flatMap((field) => resolveField(view, settings, field)) : [];

  return (
    <main className="mini-quota" data-tauri-drag-region>
      <div className="mini-title" data-tauri-drag-region="deep">
        <span>Vibe Bar</span>
        <button
          className="mini-close"
          aria-label="Hide Mini"
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
    .filter((account) => account.tool === tool && !account.error)
    .flatMap((account) => account.buckets)
    .find((candidate) => candidate.id === bucketId);
  if (!bucket) return [];
  const remaining = Math.max(0, 100 - bucket.usedPercent);
  const used = settings?.displayMode === "used";
  const bucketLabel = bucket.groupTitle ? `${bucket.groupTitle} ${bucket.title}` : bucket.title;
  return [{
    id: field,
    label: settings?.customLabels[field] || `${hierarchyFor(tool).product} ${bucketLabel}`,
    bucket,
    value: used ? bucket.usedPercent : remaining,
    suffix: used ? "used" : "left",
  }];
}

function MiniRow({ label, bucket, value, suffix }: { label: string; bucket: QuotaBucket; value: number; suffix: string }) {
  const remaining = Math.max(0, 100 - bucket.usedPercent);
  return (
    <section className="mini-row">
      <div className="mini-row-head">
        <span>{label}</span>
        {formatCountdown(bucket.resetAt) ? <small>{formatCountdown(bucket.resetAt)}</small> : null}
        <strong>{Math.round(value)}% {suffix}</strong>
      </div>
      <div className="track"><div className={`fill ${severityFor(remaining)}`} style={{ width: `${value}%` }} /></div>
    </section>
  );
}
