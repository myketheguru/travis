import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import Overlay from "./overlay/Overlay";
import { isOverlayWindow } from "./lib/overlay";
import "./styles.css";

const isOverlay = isOverlayWindow();
if (isOverlay) {
  document.body.classList.add("transparent-window");
}
const Root = isOverlay ? Overlay : App;

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Root />
  </React.StrictMode>,
);
