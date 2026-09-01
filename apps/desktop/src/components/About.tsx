import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import type { AppInfo, QuotaView } from "../api";
import { api, formatRelative } from "../api";

/**
 * Asks, and says what it found. It does not install.
 *
 * An update that arrives without being asked for is not a surprise this app
 * has any business springing on someone mid-session; the native client asks
 * first too. Which channel it looks at is `updateChannel` in Settings.
 */
function UpdateCheck() {
  const [state, setState] = useState<"idle" | "checking" | string>("idle");

  if (state === "checking") return <span className="status-line"> checking…</span>;
  if (state !== "idle") return <span className="status-line"> {state}</span>;

  return (
    <button
      type="button"
      className="link-button"
      onClick={() => {
        setState("checking");
        api
          .checkForUpdate()
          .then((version) =>
            setState(version ? `${version} is available` : "up to date"),
          )
          .catch((error: unknown) => setState(`could not check: ${String(error)}`));
      }}
    >
      Check for updates
    </button>
  );
}

export function About({ info, view }: { info: AppInfo | null; view: QuotaView | null }) {
  if (!info) return <p className="empty">Loading…</p>;

  return (
    <div className="about">
      {info.nativeApp.installed ? (
        <div className="banner">
          <span>
            <strong>The macOS native app is installed here.</strong> It has the
            full feature set today; Desktop is an early preview. Both read the
            same Vibe Bar data, and running them together is fine.
          </span>
        </div>
      ) : null}

      {info.isDemo ? (
        <div className="banner">
          <span>
            <strong>Demo mode.</strong> Reading a synthetic data root; no
            network requests and no credential access.
          </span>
        </div>
      ) : null}

      <dl>
        <dt>Version</dt>
        <dd>
          {info.version} <span className="pill">preview</span>
          <UpdateCheck />
        </dd>

        <dt>Vibe Bar data</dt>
        <dd className="mono">{info.dataRoot}</dd>

        <dt>Data written by Desktop</dt>
        <dd className="mono">{info.dataRoot}/client/desktop</dd>

        <dt>Shared data</dt>
        <dd>
          {view?.hasSharedData
            ? "Found — providers without a Desktop adapter are shown from it, marked “shared data”."
            : "None yet — Desktop shows only what it fetches itself."}
        </dd>

        <dt>Last refresh</dt>
        <dd>{formatRelative(view?.lastUpdated)}</dd>
      </dl>

      <p className="status-line" style={{ marginTop: 20, whiteSpace: "normal" }}>
        This preview fetches Codex, Claude, Alibaba, Copilot, Z.ai, MiniMax,
        Kilo, Kiro, OpenRouter, and Warp quota directly and reads every other
        provider from the shared cache. It never writes shared Vibe Bar state —
        see docs/SHARED-STORAGE.md.
      </p>

      <p style={{ marginTop: 12 }}>
        <button
          onClick={() =>
            openUrl("https://github.com/AstroQore/vibe-bar-desktop").catch(
              () => undefined,
            )
          }
        >
          Open the repository
        </button>
      </p>
    </div>
  );
}
