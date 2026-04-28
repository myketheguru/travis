import { invoke } from "@tauri-apps/api/core";

export type UpdateInfo = {
  version: string;
  notes: string | null;
  date: string | null;
};

/**
 * Hit the configured updater endpoint and return info about a pending update,
 * or null if the app is up to date.
 */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  const result = await invoke<UpdateInfo | null>("check_for_update");
  return result ?? null;
}

/**
 * Download + install the pending update and restart the app. No-op if no
 * update is available. The app will not restart unless install succeeds.
 */
export async function installUpdate(): Promise<void> {
  await invoke("install_update");
}
