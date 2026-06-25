/**
 * Lazy Python runtime IPC (v0.22.10).
 *
 * The bundle ships without Python (or with a stale Python after a
 * couple of upgrades). When a feature that needs Python is touched —
 * run_python tool, document extract, PDF rendering — we make sure the
 * runtime is in place first, downloading it on demand. UI overlay
 * shows a sleek loader keyed off `runtime-progress` events; we never
 * tell the user "downloading Python," only "Travis is getting
 * additional resources to continue."
 */
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export interface RuntimeStatus {
  /** True if a Python binary is resolvable RIGHT NOW (cache or bundle). */
  ready: boolean;
  /** True if the lazy cache exists (i.e. user has been through bootstrap). */
  cached: boolean;
  /** True if a bootstrap is currently running. */
  inProgress: boolean;
}

export interface RuntimeProgress {
  phase: "downloading" | "extracting" | "installing" | "ready" | "error";
  /** 0-100 within the current phase. The frontend interpolates between
   *  phases to render a smooth overall bar. */
  pct: number;
  message: string;
  pythonPath?: string;
  error?: string;
}

export const pythonRuntimeStatus = () =>
  invoke<RuntimeStatus>("python_runtime_status");

export const pythonRuntimeEnsure = () =>
  invoke<void>("python_runtime_ensure");

export const pythonRuntimeCancel = () =>
  invoke<void>("python_runtime_cancel");

export const pythonRuntimeEnsurePackages = (packages: string[]) =>
  invoke<void>("python_runtime_ensure_packages", { packages });

export function onRuntimeProgress(
  handler: (p: RuntimeProgress) => void,
): Promise<UnlistenFn> {
  return listen<RuntimeProgress>("runtime-progress", (event) =>
    handler(event.payload),
  );
}

/** Map a per-phase percentage to a 0-100 overall progress curve.
 *  Downloading is ~60% of the time, extracting ~10%, installing ~30%. */
export function overallProgress(p: RuntimeProgress): number {
  switch (p.phase) {
    case "downloading":
      return Math.min(60, p.pct * 0.6);
    case "extracting":
      return 60 + Math.min(10, p.pct * 0.1);
    case "installing":
      return 70 + Math.min(30, p.pct * 0.3);
    case "ready":
      return 100;
    case "error":
      return 0;
    default:
      return 0;
  }
}
