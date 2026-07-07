import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./overlay/Overlay";
import { isOverlayWindow } from "./lib/overlay";
import { AppErrorBoundary } from "./AppErrorBoundary";
import "./styles.css";

const isOverlay = isOverlayWindow();
if (isOverlay) {
  document.body.classList.add("transparent-window");
}
const Root = isOverlay ? Overlay : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <AppErrorBoundary>
      <Root />
    </AppErrorBoundary>
  </React.StrictMode>,
);
