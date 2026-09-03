import React from "react";
import ReactDOM from "react-dom/client";
// Global sheets first: the app's own sheets override them by order as well as by specificity.
import "./styles.css";
import "./popover/popover.css";
import { App } from "./App";

import { MiniQuota } from "./components/MiniQuota";
import { PopoverApp } from "./popover/PopoverRoot";

// Errors the page throws are otherwise invisible in a packaged app: no
// console, no inspector. Report them to the Rust side, which prints them.
if ("__TAURI_INTERNALS__" in window) {
  const report = (message: string) => {
    void import("@tauri-apps/api/core").then(({ invoke }) => invoke("frontend_log", { message })).catch(() => undefined);
  };
  window.addEventListener("error", (event) => report(`error: ${event.message} @ ${event.filename}:${event.lineno}:${event.colno}`));
  window.addEventListener("unhandledrejection", (event) => report(`unhandled rejection: ${String(event.reason)}`));
}

const params = new URLSearchParams(window.location.search);
const Root = params.get("mini") === "1" ? MiniQuota : params.get("popover") === "1" ? PopoverApp : App;
// The popover window is transparent and draws its own material underneath.
if (params.get("popover") === "1") document.documentElement.classList.add("popover-window");
// Outside the app, `?vibrant=1` stands in for the material Rust would provide:
// the page gets the vibrant sheets over a stand-in desktop, for looking at.
if (params.get("vibrant") === "1") document.documentElement.classList.add("vibrant", "vibrant-standin");

/**
 * Tells the shell the main page has committed its first render, so a window
 * that was waiting to be shown can appear with content instead of white.
 * A child effect runs after the whole tree above it has committed; the mini
 * and popover windows show themselves on their own terms and never send it.
 */
function Ready() {
  React.useEffect(() => {
    if (!("__TAURI_INTERNALS__" in window)) return;
    // The load generation the shell gave this page (its watchdog reloads
    // with `?boot=N`); zero on the first load.
    const generation = Number(new URLSearchParams(location.search).get("boot") ?? "0") || 0;
    void import("@tauri-apps/api/core")
      .then(({ invoke }) => invoke("frontend_ready", { generation }))
      .catch(() => undefined);
  }, []);
  return null;
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
    {Root === App ? <Ready /> : null}
  </React.StrictMode>,
);

