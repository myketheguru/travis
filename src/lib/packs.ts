import { invoke } from "@tauri-apps/api/core";

export type PackInfo = {
  slug: string;
  name: string;
  description: string;
  version: string;
  enabled: boolean;
};

/** Every compiled-in pack with its current runtime-enabled state. */
export const listPacks = () => invoke<PackInfo[]>("list_packs");

/** Toggle a pack's `meta.pack.<slug>.enabled` flag. Takes effect on
 * next launch — surface a "Restart Travis" hint after calling. */
export const setPackEnabled = (slug: string, enabled: boolean) =>
  invoke<void>("set_pack_enabled", { slug, enabled });
