import { useEffect, useState } from "react";

import type { SkillsInventoryView } from "../api";
import { api, formatRelative } from "../api";

const TARGET_LABELS: Record<string, string> = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  antigravity: "AntiGravity",
  grok: "Grok Build",
  cursor: "Cursor",
};

export function Skills() {
  const [view, setView] = useState<SkillsInventoryView | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = () => {
    setLoading(true);
    api
      .skillsInventory()
      .then(setView)
      .catch(() => setView(null))
      .finally(() => setLoading(false));
  };

  useEffect(() => {
    refresh();
  }, []);

  return (
    <section>
      <div className="toolbar">
        <h2 style={{ margin: 0 }}>Skills</h2>
        <span className="status-line" style={{ marginLeft: "auto" }}>
          {view ? `${view.skills.length} found · scanned ${formatRelative(view.scannedAt)}` : "not scanned"}
        </span>
        <button onClick={refresh} disabled={loading}>
          {loading ? "Scanning…" : "Refresh"}
        </button>
      </div>

      {view?.warnings.map((warning) => (
        <p className="error-row" key={warning}>{warning}</p>
      ))}

      {!view ? (
        <p className="empty">{loading ? "Scanning local skills…" : "Unable to scan local skills."}</p>
      ) : view.skills.length === 0 ? (
        <p className="empty">No installed skills found.</p>
      ) : (
        view.skills.map((skill) => (
          <article className="card" key={skill.directory}>
            <div className="card-head">
              <span className="card-title">{skill.name}</span>
              <span className="pill">{skill.health.replaceAll("_", " ")}</span>
            </div>
            {skill.description ? <p>{skill.description}</p> : null}
            <p className="status-line" style={{ whiteSpace: "normal", marginBottom: 0 }}>
              {skill.source} · {skill.targets.length
                ? skill.targets.map((target) => TARGET_LABELS[target] ?? target).join(", ")
                : "no verified projection"}
            </p>
          </article>
        ))
      )}
    </section>
  );
}
