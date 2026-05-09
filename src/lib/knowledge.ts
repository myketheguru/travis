import { invoke } from "@tauri-apps/api/core";

/// One row of the cross-pack entity list returned by
/// `list_entities_by_family`. Backend is identity_cmd::EntityListRow.
export type EntityListRow = {
  id: number;
  kind: string;
  displayName: string;
  packSlug: string | null;
  mentionsCount: number;
  lastSeen: string;
  confidence: number;
  workspaceId: number;
};

/// Entity-kind family for the Knowledge tabs. Each family covers a
/// stable set of pack-declared kinds plus the matching `<family>:*`
/// ambient kinds. Stays stable as packs add new role names.
export type KnowledgeFamily = "person" | "place" | "org";

export const listEntitiesByFamily = (family: KnowledgeFamily, limit = 200) =>
  invoke<EntityListRow[]>("list_entities_by_family", { family, limit });
