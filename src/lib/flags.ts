import { invoke } from "@tauri-apps/api/core";

export type Flags = {
  features?: Record<string, unknown>;
  [key: string]: unknown;
};

/**
 * Force a fetch of remote flags. On network failure the backend returns the
 * cached flags (or empty defaults), so this never throws for "offline".
 */
export async function refreshFlags(): Promise<Flags> {
  return invoke<Flags>("refresh_flags");
}

/**
 * Read locally cached flags. Returns `{ features: {} }` if nothing has been
 * fetched yet.
 */
export async function getFlags(): Promise<Flags> {
  return invoke<Flags>("get_flags");
}

/**
 * Convenience: read a single flag from `cached.features[name]`. Returns null
 * if not present.
 */
export async function getFlag<T = unknown>(name: string): Promise<T | null> {
  const v = await invoke<T | null>("get_flag", { name });
  return v ?? null;
}

/**
 * Override the URL the backend pulls remote config from. Persisted in
 * `meta.flags_url`.
 */
export async function setFlagsUrl(url: string): Promise<void> {
  await invoke("set_flags_url", { url });
}
