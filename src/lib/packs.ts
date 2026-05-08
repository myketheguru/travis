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

// ---------------------------------------------------------------------------
// Plugin platform — schema metadata + auto-CRUD (PLUGIN_PLATFORM.md)
// ---------------------------------------------------------------------------

export type FieldType =
  | { kind: "text" }
  | { kind: "longText" }
  | { kind: "email" }
  | { kind: "phone" }
  | { kind: "integer" }
  | { kind: "number" }
  | { kind: "currency" }
  | { kind: "date" }
  | { kind: "dateTime" }
  | { kind: "bool" }
  | { kind: "enum"; options: string[] }
  | { kind: "ref"; table: string }
  | { kind: "json" }
  | { kind: "timestamp" };

export type FieldDef = {
  slug: string;
  label: string;
  fieldType: FieldType;
  required: boolean;
  help: string | null;
  defaultInList: boolean;
};

export type SortDir = "asc" | "desc";

export type ListViewDef = {
  columns: string[];
  defaultSort: string | null;
  defaultSortDir: SortDir;
  pageSize: number;
};

export type TableDef = {
  slug: string;
  displayName: string;
  singularName: string;
  displayField: string;
  entityKind: string | null;
  fields: FieldDef[];
  primary: boolean;
  listView: ListViewDef;
};

export type PackSchema = {
  slug: string;
  name: string;
  tables: TableDef[];
};

/** Schema metadata for every enabled pack. Drives the auto-CRUD UI. */
export const packSchemas = () => invoke<PackSchema[]>("pack_schemas");

export type TableListParams = {
  packSlug: string;
  tableSlug: string;
  sort?: string;
  sortDir?: SortDir;
  limit?: number;
  offset?: number;
};

/** Auto-CRUD list. Returns each row as a flat object keyed by field slug. */
export const packTableList = (params: TableListParams) =>
  invoke<Record<string, unknown>[]>("pack_table_list", { params });

export type TableGetParams = {
  packSlug: string;
  tableSlug: string;
  id: number;
};

/** Auto-CRUD single-row fetch. Throws if the row doesn't exist. */
export const packTableGet = (params: TableGetParams) =>
  invoke<Record<string, unknown>>("pack_table_get", { params });

export type TableUpsertParams = {
  packSlug: string;
  tableSlug: string;
  payload: Record<string, unknown>;
};

/** Auto-CRUD insert / update. If `payload.id` is present and non-null,
 * the row is updated; otherwise a new row is inserted. Returns the
 * resulting row. */
export const packTableUpsert = (params: TableUpsertParams) =>
  invoke<Record<string, unknown>>("pack_table_upsert", { params });

export type TableDeleteParams = {
  packSlug: string;
  tableSlug: string;
  id: number;
};

/** Auto-CRUD delete. Cascades follow the SQL FK definitions. */
export const packTableDelete = (params: TableDeleteParams) =>
  invoke<void>("pack_table_delete", { params });
