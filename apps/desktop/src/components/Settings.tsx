import type { ReactNode } from "react";

import type { PresentationSettings } from "../api";
import { subProviderFor } from "../naming";

export function Settings({ settings }: { settings: PresentationSettings | null }) {
  if (!settings) return <p className="empty">Loading shared presentation settings…</p>;

  const fieldLabel = (fieldId: string) => settings.customLabels[fieldId] || fieldId;
  return (
    <section className="settings-readonly">
      <p className="banner">
        These preferences are managed by Vibe Bar’s shared settings. Desktop reads
        and applies them but does not save changes here.
      </p>

      <SettingsGroup title="Display">
        <Setting name="Percent shows" value={settings.displayMode === "used" ? "Used" : "Remaining"} />
        <Setting name="Refresh interval" value={formatInterval(settings.refreshIntervalSeconds)} />
        <Setting name="Menu-bar colour basis" value={settings.menuBarColorBasis || "Actual"} />
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

function formatInterval(seconds: number): string {
  if (seconds % 3600 === 0) return `${seconds / 3600}h`;
  if (seconds % 60 === 0) return `${seconds / 60}m`;
  return `${seconds}s`;
}

function providerList(tools: string[]): string {
  return tools.length ? tools.map((tool) => subProviderFor(tool)).join(", ") : "None";
}
