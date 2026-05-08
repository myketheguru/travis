import { invoke } from "@tauri-apps/api/core";

export type WorkspaceCategory =
  | "work"
  | "personal"
  | "health"
  | "therapy"
  | "legal"
  | "finance"
  | "other";

export type Workspace = {
  id: number;
  slug: string;
  name: string;
  category: WorkspaceCategory;
  /** SQLite stores BOOL as INTEGER 0/1. */
  crossVisible: number;
  archivedAt: string | null;
  createdAt: string;
  updatedAt: string;
};

export type WorkspaceInput = {
  slug?: string;
  name: string;
  category?: WorkspaceCategory;
  crossVisible?: boolean;
};

export type ActiveWorkspaceInfo = {
  workspace: Workspace;
  visibleIds: number[];
};

export type WorkspaceChangeEvent = {
  active: Workspace;
  visibleIds: number[];
};

export const SENSITIVE_CATEGORIES: WorkspaceCategory[] = [
  "health",
  "therapy",
  "legal",
  "finance",
];

export const isSensitive = (cat: WorkspaceCategory): boolean =>
  SENSITIVE_CATEGORIES.includes(cat);

export const listWorkspaces = () => invoke<Workspace[]>("list_workspaces");

export const getActiveWorkspace = () =>
  invoke<ActiveWorkspaceInfo>("get_active_workspace");

export const setActiveWorkspaceCmd = (id: number) =>
  invoke<ActiveWorkspaceInfo>("set_active_workspace", { id });

export const createWorkspace = (input: WorkspaceInput) =>
  invoke<Workspace>("create_workspace", { input });

export const updateWorkspace = (id: number, input: WorkspaceInput) =>
  invoke<Workspace>("update_workspace", { id, input });

export const archiveWorkspace = (id: number) =>
  invoke<Workspace>("archive_workspace", { id });

export const unarchiveWorkspace = (id: number) =>
  invoke<Workspace>("unarchive_workspace", { id });
