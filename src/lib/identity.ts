import { invoke } from "@tauri-apps/api/core";

export type EntityKind = "coach" | "school" | "dept";

export type EntityIndex = {
  id: number;
  kind: EntityKind;
  normalizedName: string;
  displayName: string;
  mentionsCount: number;
  firstSeen: string;
  lastSeen: string;
  attributesJson: string | null;
};

export const listEntities = (kind?: EntityKind, limit?: number) =>
  invoke<EntityIndex[]>("list_entities", { kind, limit });

export const getProfileBlurb = () => invoke<string>("get_profile_blurb");
