import React from "react";
import ReactDOM from "react-dom/client";
// Global sheets first: the app's own sheets override them by order as well as by specificity.
import "./styles.css";
import "./popover/popover.css";
import { App } from "./App";

import { MiniQuota } from "./components/MiniQuota";
import { PopoverApp } from "./popover/PopoverRoot";

const params = new URLSearchParams(window.location.search);
const Root = params.get("mini") === "1" ? MiniQuota : params.get("popover") === "1" ? PopoverApp : App;
// The popover window is transparent and draws its own material underneath.
if (params.get("popover") === "1") document.documentElement.classList.add("popover-window");
// Outside the app, `?vibrant=1` stands in for the material Rust would provide:
// the page gets the vibrant sheets over a stand-in desktop, for looking at.
if (params.get("vibrant") === "1") document.documentElement.classList.add("vibrant", "vibrant-standin");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
