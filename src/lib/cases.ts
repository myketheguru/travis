import { invoke } from "@tauri-apps/api/core";

export interface Case {
  id: number;
  workspaceId: number;
  name: string;
  summary: string | null;
  status: "open" | "paused" | "closed";
  parentCaseId: number | null;
  conversationIdsJson: string;
  startedAt: string;
  lastActivityAt: string;
  closedAt: string | null;
}

export interface CaseArtifact {
  id: number;
  caseId: number;
  kind: string;
  payloadJson: string;
  documentId: number | null;
  createdAt: string;
}

export const listOpenCases = (limit?: number) =>
  invoke<Case[]>("list_open_cases", { limit });

export const closeCase = (id: number) =>
  invoke<void>("close_case", { id });

export const caseForConversation = (conversationId: number) =>
  invoke<Case | null>("case_for_conversation", { conversationId });

export const listCaseArtifacts = (caseId: number, limit?: number) =>
  invoke<CaseArtifact[]>("list_case_artifacts", { caseId, limit });
