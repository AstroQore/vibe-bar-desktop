import type { ChipGroup, TrendBucket, UsageStatsView } from "../../api";
import { Calendar, XCircle, Refresh } from "../icons";
import { ToolBrandIcon } from "../../popover/brand";
import { Menu, MenuItem } from "./Menu";
import {
  RANGE_PRESETS,
  REFRESH_INTERVALS,
  type CustomRange,
  type RangePreset,
  type RefreshInterval,
  modelSummary,
  rangeSummary,
  refreshMenuTitle,
  refreshTitle,
  toggleCompany,
  toggleHarness,
} from "./model";

export interface FilterState {
  preset: RangePreset;
  custom?: CustomRange;
  harnesses: string[] | null;
  models: string[] | null;
  granularity: TrendBucket | null;
  refreshInterval: RefreshInterval;
}

function Cpu({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
      <rect x="4" y="4" width="8" height="8" rx="1.5" />
      <rect x="6.3" y="6.3" width="3.4" height="3.4" rx="0.6" />
      <path d="M6 1.5v2.5M10 1.5v2.5M6 12v2.5M10 12v2.5M1.5 6h2.5M1.5 10h2.5M12 6h2.5M12 10h2.5" />
    </svg>
  );
}

function Pause({ size = 12 }: { size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
      <circle cx="8" cy="8" r="6.2" />
      <path d="M6.3 5.6v4.8M9.7 5.6v4.8" />
    </svg>
  );
}

/** Brand icon ids and chip tints per billing company; harnesses reuse their
 *  company's tint the way the native chips do. */
const COMPANY_TOOL: Record<string, string> = { OpenAI: "codex", Anthropic: "claude", "Google AI": "gemini", SpaceXAI: "grok" };
const HARNESS_TOOL: Record<string, string> = { Codex: "codex", "Claude Code": "claude", "Gemini Web": "gemini", AntiGravity: "antigravity", Grok: "grok", Cursor: "cursor" };
const COMPANY_TINT: Record<string, string> = { OpenAI: "#5F8F7A", Anthropic: "#D97757", "Google AI": "#4285F4", SpaceXAI: "#6E6E73" };

function tint(company: string): string {
  return COMPANY_TINT[company] ?? "var(--wb-accent)";
}

function toLocalInput(at: number): string {
  const date = new Date(at * 1000);
  const pad = (v: number) => (v < 10 ? `0${v}` : `${v}`);
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}T${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** The native `UsageFiltersBar`: harness chips grouped under their company,
 *  then the range, models, and auto-refresh menus. */
export function FiltersBar({
  state,
  view,
  now,
  onChange,
}: {
  state: FilterState;
  view: UsageStatsView | null;
  now: number;
  onChange: (next: FilterState) => void;
}) {
  const groups: ChipGroup[] = view?.chipGroups ?? [];
  const all = groups.flatMap((group) => group.harnesses);
  const selected = state.harnesses;
  const allOn = selected === null;
  const isOn = (harness: string) => selected === null || selected.includes(harness);
  const filtered = selected !== null || state.models !== null || state.preset !== "day7";
  const range = view ? rangeSummary(state.preset, view.rangeStart, view.rangeEnd) : "…";
  const custom = state.custom ?? { start: now - 7 * 86_400, end: now };

  return (
    <div className="wb-toolbar us-filters">
      <div className="us-chips" role="group" aria-label="Harness filter">
        <button
          type="button"
          className={`wb-pill us-pill-all${allOn ? " on" : ""}`}
          title={allOn ? "Click to select no harness" : "Click to include every harness"}
          onClick={() => onChange({ ...state, harnesses: allOn ? [] : null, models: null })}
        >
          All harnesses
        </button>
        {groups.map((group) => {
          const every = group.harnesses.every(isOn);
          const some = group.harnesses.some(isOn);
          const color = tint(group.company);
          return (
            <div className="us-chip-group" key={group.company}>
              <button
                type="button"
                className={`wb-pill us-pill-company${every ? " on" : some ? " partial" : ""}`}
                style={{ "--wb-pill-tint": color } as React.CSSProperties}
                title={`${group.company} · ${group.harnesses.join(" + ")}`}
                onClick={() => onChange({ ...state, harnesses: toggleCompany(selected, all, group.harnesses), models: null })}
              >
                {COMPANY_TOOL[group.company] ? <ToolBrandIcon tool={COMPANY_TOOL[group.company]} size={11} opacity={0.8} /> : null}
                {group.company}
              </button>
              {group.harnesses.map((harness) => (
                <button
                  type="button"
                  key={harness}
                  className={`wb-pill us-pill-harness${isOn(harness) ? " on" : ""}`}
                  style={{ "--wb-pill-tint": color } as React.CSSProperties}
                  onClick={() => onChange({ ...state, harnesses: toggleHarness(selected, all, harness), models: null })}
                >
                  {HARNESS_TOOL[harness] ? <ToolBrandIcon tool={HARNESS_TOOL[harness]} size={11} /> : null}
                  {harness}
                </button>
              ))}
            </div>
          );
        })}
      </div>
      <div className="us-filters-row">
      <Menu icon={<Calendar size={12} />} title={RANGE_PRESETS.find((p) => p.id === state.preset)?.title ?? "Range"} detail={range} ariaLabel="Choose the date range" width={236}>
        {(close) => (
          <>
            {RANGE_PRESETS.map((preset) => (
              <MenuItem
                key={preset.id}
                checked={state.preset === preset.id}
                onSelect={() => {
                  onChange({ ...state, preset: preset.id, custom: preset.id === "custom" ? custom : state.custom });
                  if (preset.id !== "custom") close();
                }}
              >
                {preset.title}
              </MenuItem>
            ))}
            {state.preset === "custom" ? (
              <div className="us-custom-range">
                <div className="wb-label-caps">Custom range</div>
                <label>
                  <span>From</span>
                  <input
                    type="datetime-local"
                    value={toLocalInput(custom.start)}
                    onChange={(event) => onChange({ ...state, custom: { ...custom, start: new Date(event.target.value).getTime() / 1000 } })}
                  />
                </label>
                <label>
                  <span>To</span>
                  <input
                    type="datetime-local"
                    value={toLocalInput(custom.end)}
                    onChange={(event) => onChange({ ...state, custom: { ...custom, end: new Date(event.target.value).getTime() / 1000 } })}
                  />
                </label>
                <p className="us-hint">Choose hourly, daily, or weekly buckets from the chart toolbar.</p>
              </div>
            ) : null}
          </>
        )}
      </Menu>
      <Menu icon={<Cpu />} title="Models" detail={modelSummary(state.models, view?.availableModels ?? [])} ariaLabel="Choose which models to include" width={280}>
        {() => {
          const available = view?.availableModels ?? [];
          return (
            <>
              <MenuItem checked={state.models === null} onSelect={() => onChange({ ...state, models: null })}>
                All models
              </MenuItem>
              {available.length === 0 ? <div className="us-menu-empty">No models in range</div> : null}
              {available.map((model) => {
                const on = state.models === null || state.models.includes(model);
                return (
                  <MenuItem
                    key={model}
                    checked={on}
                    onSelect={() => {
                      const current = state.models ?? available;
                      const next = on ? current.filter((m) => m !== model) : [...current, model];
                      onChange({ ...state, models: available.every((m) => next.includes(m)) ? null : next });
                    }}
                  >
                    {model}
                  </MenuItem>
                );
              })}
            </>
          );
        }}
      </Menu>
      <Menu
        icon={state.refreshInterval === 0 ? <Pause /> : <Refresh size={12} />}
        title="Auto"
        detail={refreshTitle(state.refreshInterval)}
        ariaLabel="Choose how often the page re-queries"
        width={160}
      >
        {(close) => (
          <>
            {REFRESH_INTERVALS.map((interval) => (
              <MenuItem
                key={interval}
                checked={state.refreshInterval === interval}
                onSelect={() => {
                  onChange({ ...state, refreshInterval: interval });
                  close();
                }}
              >
                {refreshMenuTitle(interval)}
              </MenuItem>
            ))}
          </>
        )}
      </Menu>
        {filtered ? (
          <button
            type="button"
            className="us-clear"
            onClick={() => onChange({ ...state, preset: "day7", custom: undefined, harnesses: null, models: null })}
          >
            <XCircle size={12} /> Clear
          </button>
        ) : null}
      </div>
    </div>
  );
}
