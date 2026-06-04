import { invoke } from "@tauri-apps/api/core";

export interface Document {
  id: number;
  workspaceId: number;
  kind: string;
  displayName: string;
  originalFilename: string;
  contentHash: string;
  relativePath: string;
  sizeBytes: number;
  mimeType: string;
  ingestStatus: "pending" | "extracted" | "failed" | "skipped";
  extractedJson?: string | null;
  extractionError?: string | null;
  source: "user_dropped" | "generated_by_travis" | "imported";
  conversationId?: number | null;
  workflowStateId?: number | null;
  createdAt: string;
  updatedAt: string;
}

export interface IngestDocumentParams {
  filePath: string;
  kind?: string;
  displayName?: string;
  conversationId?: number | null;
  workflowStateId?: number | null;
}

export interface ListDocumentsFilter {
  workspaceId?: number | null;
  kind?: string | null;
  conversationId?: number | null;
  workflowStateId?: number | null;
  entityId?: number | null;
  limit?: number | null;
}

export async function ingestDocument(
  params: IngestDocumentParams,
): Promise<Document> {
  return await invoke<Document>("ingest_document", { params });
}

export async function listDocuments(
  filter?: ListDocumentsFilter,
): Promise<Document[]> {
  return await invoke<Document[]>("list_documents", { filter: filter ?? null });
}

export async function getDocument(id: number): Promise<Document | null> {
  return await invoke<Document | null>("get_document", { id });
}

export async function getDocumentPath(id: number): Promise<string> {
  return await invoke<string>("get_document_path", { id });
}

export async function linkDocument(
  documentId: number,
  entityId: number,
  relationKind?: string,
): Promise<void> {
  await invoke("link_document", {
    params: {
      documentId,
      entityId,
      relationKind: relationKind ?? null,
    },
  });
}

export async function setDocumentKind(
  documentId: number,
  kind: string,
): Promise<void> {
  await invoke("set_document_kind", {
    params: { documentId, kind },
  });
}

export async function deleteDocument(id: number): Promise<void> {
  await invoke("delete_document", { id });
}

/// Open the document with the OS default viewer (Preview, Acrobat,
/// browser, etc.). Returns the absolute path that was opened.
export async function previewDocument(id: number): Promise<string> {
  return await invoke<string>("preview_document", { id });
}

/// Format a byte count as a human-readable size for the UI ("1.2 MB").
export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB"];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v.toFixed(1)} ${units[i]}`;
}
