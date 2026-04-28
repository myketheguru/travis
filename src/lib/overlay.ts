import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

export const isOverlayWindow = () => getCurrentWindow().label === "overlay";

export const hideOverlay = () => invoke<void>("hide_overlay");
export const toggleOverlay = () => invoke<void>("toggle_overlay");
