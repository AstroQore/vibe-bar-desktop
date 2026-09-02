import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { MiniQuota } from "./components/MiniQuota";
import { PopoverApp } from "./popover/PopoverRoot";
import "./styles.css";
import "./popover/popover.css";

const params = new URLSearchParams(window.location.search);
const Root = params.get("mini") === "1" ? MiniQuota : params.get("popover") === "1" ? PopoverApp : App;
// The popover window is transparent and draws its own material underneath.
if (params.get("popover") === "1") document.documentElement.classList.add("popover-window");

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
