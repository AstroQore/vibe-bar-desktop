import type { ReactNode } from "react";

import type { PresentationSettings } from "../api";
import { humanisedSettingName } from "../settingNames";
import { subProviderFor } from "../naming";

/** The refresh cadences the native app offers, so the two Settings windows
 *  present the same choices rather than each inventing a list. */
const REFRESH_INTERVALS = [60, 120, 300, 600, 900, 1800, 3600];

export function Settings({
  settings,
  replacedKeys,
  saveError,
  onSave,
  onDismissReplaced,
}: {
  settings: PresentationSettings | null;
  replacedKeys: string[] | null;
  saveError: string | null;
  onSave: (changes: Record<string, unknown>) => void;
  onDismissReplaced: () => void;
}) {
  if (!settings) return <p className="empty">Loading shared presentation settings…</p>;

  const fieldLabel = (fieldId: string) => settings.customLabels[fieldId] || fieldId;
  return (
    <section className="settings-readonly">
      {saveError ? (
        <div className="banner banner-warning">
          <div>
            <strong>That setting was not saved</strong>
            <p>{saveError}</p>
          </div>
        </div>
      ) : null}
      {replacedKeys?.length ? (
        <div className="banner banner-warning">
          <div>
            <strong>Another Vibe Bar replaced your change</strong>
            <p>{replacedSummary(replacedKeys)}</p>
          </div>
          <button type="button" onClick={onDismissReplaced}>
            Dismiss
          </button>
        </div>
      ) : (
        <p className="banner">
          These preferences are shared with the Vibe Bar menu-bar app. Changes made
          here reach it, and changes it makes reach here.
        </p>
      )}

      <SettingsGroup title="Display">
        <div className="setting-row">
          <span>Percent shows</span>
          <select
            value={settings.displayMode === "used" ? "used" : "remaining"}
            onChange={(event) => onSave({ displayMode: event.target.value })}
          >
            <option value="remaining">Remaining</option>
            <option value="used">Used</option>
          </select>
        </div>
        <div className="setting-row">
          <span>Refresh interval</span>
          <select
            value={String(settings.refreshIntervalSeconds)}
            onChange={(event) =>
              onSave({ refreshIntervalSeconds: Number(event.target.value) })
            }
          >
            {/* A cadence the native app set but this list does not offer is
                still the current one, and must not silently become another. */}
            {(REFRESH_INTERVALS.includes(settings.refreshIntervalSeconds)
              ? REFRESH_INTERVALS
              : [...REFRESH_INTERVALS, settings.refreshIntervalSeconds].sort((a, b) => a - b)
            ).map((seconds) => (
              <option key={seconds} value={String(seconds)}>
                {formatInterval(seconds)}
              </option>
            ))}
          </select>
        </div>
        <div className="setting-row">
          <span>Menu-bar colour basis</span>
          <select
            value={settings.menuBarColorBasis || "actual"}
            onChange={(event) => onSave({ menuBarColorBasis: event.target.value })}
          >
            <option value="actual">Actual</option>
            <option value="forecast">Forecast</option>
          </select>
        </div>
      </SettingsGroup>

      <SettingsGroup title="Overview">
        <Setting name="Core order" value={providerList(settings.coreProviderOrder)} />
        <Setting
          name="Visible core providers"
          value={settings.visibleCoreProviders ? providerList(settings.visibleCoreProviders) : "All"}
        />
        <Setting
          name="Visible misc providers"
          value={settings.visibleMiscProviders ? providerList(settings.visibleMiscProviders) : "All"}
        />
      </SettingsGroup>

      <SettingsGroup title="Menu-bar fields">
        {settings.selectedFieldIds.length ? (
          <ul className="settings-list">
            {settings.selectedFieldIds.map((fieldId) => (
              <li key={fieldId}>
                <span>{fieldLabel(fieldId)}</span>
                {settings.customLabels[fieldId] ? <code>{fieldId}</code> : null}
              </li>
            ))}
          </ul>
        ) : (
          <p className="status-line">Desktop uses its default fields until the native app saves a selection.</p>
        )}
      </SettingsGroup>

      <SettingsGroup title="Plan labels">
        {Object.keys(settings.providerPlanLabels).length ? (
          <ul className="settings-list">
            {Object.entries(settings.providerPlanLabels).map(([tool, label]) => (
              <li key={tool}>
                <span>{subProviderFor(tool)}</span>
                <span className="pill">{label}</span>
              </li>
            ))}
          </ul>
        ) : (
          <p className="status-line">No custom plan labels.</p>
        )}
      </SettingsGroup>
    </section>
  );
}

function SettingsGroup({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="settings-group">
      <h2>{title}</h2>
      {children}
    </section>
  );
}

function Setting({ name, value }: { name: string; value: string }) {
  return (
    <div className="setting-row">
      <span>{name}</span>
      <span>{value}</span>
    </div>
  );
}

/** The one sentence under the notice. */
export function replacedSummary(keys: string[]): string {
  const names = keys.map(humanisedSettingName);
  if (!names.length) return "";
  const listed = names.slice(0, 3).join(", ");
  if (names.length > 3) {
    return `${listed} and ${names.length - 3} more settings now hold the other copy's value.`;
  }
  return `${listed} now ${names.length === 1 ? "holds" : "hold"} the other copy's value.`;
}

function formatInterval(seconds: number): string {
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}

function providerList(tools: string[]): string {
  return tools.length ? tools.map((tool) => subProviderFor(tool)).join(", ") : "None";
}
