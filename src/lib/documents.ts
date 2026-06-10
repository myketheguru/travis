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

/// Trigger (or re-trigger) extraction on a document. `force=true`
/// resets status and re-runs even if already extracted.
export async function extractDocument(
  documentId: number,
  force = false,
): Promise<Document> {
  return await invoke<Document>("extract_document", {
    params: { documentId, force },
  });
}

/// Overwrite a document's extracted JSON wholesale. Used by the
/// confirmation card after the user edits fields inline.
export async function updateDocumentExtraction(
  documentId: number,
  extractedJson: unknown,
): Promise<Document> {
  return await invoke<Document>("update_document_extraction", {
    params: { documentId, extractedJson },
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

/// v0.18.3 — reveal the document in the OS file explorer (Finder /
/// Explorer / your distro's file manager). Different from previewDocument
/// which opens the file directly; this surfaces WHERE on disk it lives.
/// Returns the absolute path.
export async function revealDocumentInFolder(id: number): Promise<string> {
  return await invoke<string>("reveal_document_in_folder", { id });
}

/// v0.20.1 — download a managed document to a user-chosen location
/// via the OS save dialog. Default filename is the original filename
/// the user dropped (or Travis generated). Returns the resolved
/// target path, or null if the user cancelled the dialog.
export async function downloadDocument(
  id: number,
  defaultFilename: string,
): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const target = await save({
    defaultPath: defaultFilename,
  });
  if (!target) return null;
  return await invoke<string>("download_document", {
    id,
    targetPath: target,
  });
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
