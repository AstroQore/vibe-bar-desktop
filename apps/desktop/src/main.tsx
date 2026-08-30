import React from "react";
import ReactDOM from "react-dom/client";

import { App } from "./App";
import { MiniQuota } from "./components/MiniQuota";
import "./styles.css";

const Root = new URLSearchParams(window.location.search).get("mini") === "1" ? MiniQuota : App;

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
