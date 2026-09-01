import { useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";

import type { AppInfo, PendingUpdate, QuotaView } from "../api";
import { api, formatRelative } from "../api";

/**
 * Asks, then installs if told to.
 *
 * Two steps, because installing replaces the running application and restarts
 * it. An update that arrives without being asked for is not a surprise this
 * app has any business springing on someone mid-session; the native client
 * asks first too. Which channel it looks at is `updateChannel` in Settings.
 *
 * Every state that is not in flight offers a way onwards, including the two
 * failures: a check that could not reach the feed and an install that could
 * not finish are both worth trying again, and a dead end would mean quitting
 * the app to get another go.
 */
function UpdateCheck() {
  const [state, setState] = useState<
    | { at: "checking" }
    | { at: "installing" }
    | { at: "idle"; note?: string }
    | { at: "found"; update: PendingUpdate; note?: string }
  >({ at: "idle" });

  if (state.at === "checking") return <span className="status-line"> checking…</span>;
  if (state.at === "installing")
    return <span className="status-line"> downloading and installing…</span>;

  const note = state.note ? <span className="status-line"> {state.note}</span> : null;

  if (state.at === "found") {
    const { update } = state;
    return (
      <span className="status-line">
        {" "}
        {update.version} is available{" "}
        <button
          type="button"
          className="link-button"
          onClick={() => {
            setState({ at: "installing" });
            api.installUpdate(update.id).catch((error: unknown) =>
              // Back to the same offer: the backend kept the update, so this
              // is a retry rather than a fresh check.
              setState({
                at: "found",
                update,
                note: `could not install: ${String(error)}`,
              }),
            );
          }}
        >
          Install and restart
        </button>
        {note}
      </span>
    );
  }

  return (
    <>
      <button
        type="button"
        className="link-button"
        onClick={() => {
          setState({ at: "checking" });
          api
            .checkForUpdate()
            .then((update) =>
              setState(
                update ? { at: "found", update } : { at: "idle", note: "up to date" },
              ),
            )
            .catch((error: unknown) =>
              setState({ at: "idle", note: `could not check: ${String(error)}` }),
            );
        }}
      >
        Check for updates
      </button>
      {note}
    </>
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
          {info.isDemo ? null : <UpdateCheck />}
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
