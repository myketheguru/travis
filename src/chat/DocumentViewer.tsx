/**
 * v0.20.1 — in-Travis split-window document previewer.
 *
 * When `viewerDocumentId` is set in the app store, Manage renders
 * this panel on the right and the chat on the left, separated by a
 * resizable handle. Multi-format body:
 *   - PDF             → <iframe> with convertFileSrc URL
 *   - image           → <img>
 *   - text/CSV/code   → fetched + rendered in a monospaced container
 *   - other           → fallback open-externally message
 *
 * Closes when the user clicks the X — viewerDocumentId returns to
 * null and Manage falls back to single-pane chat.
 */
import { useEffect, useMemo, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import {
  formatBytes,
  getDocument,
  getDocumentPath,
  previewDocument,
  revealDocumentInFolder,
  downloadDocument,
  type Document,
} from "../lib/documents";
import { DocumentIcon } from "./DocumentIcon";
import { useAppStore } from "../stores/app";

interface Props {
  documentId: number;
}

export function DocumentViewer({ documentId }: Props) {
  const setViewerDocumentId = useAppStore((s) => s.setViewerDocumentId);
  const docFullscreen = useAppStore((s) => s.docFullscreen);
  const setDocFullscreen = useAppStore((s) => s.setDocFullscreen);
  const [doc, setDoc] = useState<Document | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [textBody, setTextBody] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setDoc(null);
    setTextBody(null);
    setError(null);
    getDocument(documentId)
      .then((d) => {
        if (cancelled) return;
        if (!d) setError("Document not found.");
        else setDoc(d);
      })
      .catch((e) => !cancelled && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      cancelled = true;
    };
  }, [documentId]);

  const assetUrl = useMemo(() => {
    if (!doc) return null;
    // We have the document's relative_path under the storage root; the
    // backend exposes the absolute path via get_document_path. Build
    // the Tauri asset URL once that's resolved.
    return null;
  }, [doc]);
  // Resolve the asset URL via the backend path command.
  const [resolvedUrl, setResolvedUrl] = useState<string | null>(null);
  useEffect(() => {
    if (!doc) {
      setResolvedUrl(null);
      return;
    }
    let cancelled = false;
    getDocumentPath(doc.id)
      .then((p) => {
        if (!cancelled) setResolvedUrl(convertFileSrc(p));
      })
      .catch(() => {
        if (!cancelled) setResolvedUrl(null);
      });
    return () => {
      cancelled = true;
    };
  }, [doc]);
  void assetUrl;

  const bodyKind = useMemo<BodyKind>(() => {
    if (!doc) return "unknown";
    const m = (doc.mimeType || "").toLowerCase();
    const k = (doc.kind || "").toLowerCase();
    if (m === "application/pdf" || k.includes("pdf")) return "pdf";
    if (m.startsWith("image/")) return "image";
    if (
      m.startsWith("text/") ||
      m === "application/json" ||
      m === "application/xml" ||
      k === "txt" ||
      k === "md" ||
      k === "csv" ||
      k === "generated_csv" ||
      k === "code"
    )
      return "text";
    return "unknown";
  }, [doc]);

  // Fetch text body for text-shaped docs once the asset URL is known.
  useEffect(() => {
    if (!doc || bodyKind !== "text" || !resolvedUrl) return;
    let cancelled = false;
    fetch(resolvedUrl)
      .then((r) => r.text())
      .then((t) => {
        if (cancelled) return;
        // Cap to a sane size so a giant log file doesn't murder the
        // browser; user can still download / open externally for full
        // content.
        setTextBody(t.length > 1_000_000 ? t.slice(0, 1_000_000) + "\n\n…(truncated)" : t);
      })
      .catch(() => {
        if (!cancelled) setTextBody(null);
      });
    return () => {
      cancelled = true;
    };
  }, [doc, bodyKind, resolvedUrl]);

  return (
    <div className="h-full w-full flex flex-col bg-ink">
      <header className="flex items-center gap-2 px-3 py-2 border-b border-white/[0.06] shrink-0">
        <span className="text-bone-2 shrink-0">
          <DocumentIcon kind={doc?.kind} mimeType={doc?.mimeType} size={16} />
        </span>
        <div className="flex-1 min-w-0">
          <div className="text-bone text-[12.5px] truncate">
            {doc?.displayName ?? "Loading…"}
          </div>
          {doc && (
            <div className="text-bone-3 text-[10px] font-mono">
              {doc.kind} · {formatBytes(doc.sizeBytes)}
            </div>
          )}
        </div>
        {doc && (
          <>
            <IconBtn
              label="Download"
              onClick={() => downloadDocument(doc.id, doc.originalFilename).catch(() => {})}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
                <polyline points="7 10 12 15 17 10" />
                <line x1="12" y1="15" x2="12" y2="3" />
              </svg>
            </IconBtn>
            <IconBtn
              label="Show in file manager"
              onClick={() => revealDocumentInFolder(doc.id).catch(() => {})}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
              </svg>
            </IconBtn>
            <IconBtn
              label="Open in OS viewer"
              onClick={() => previewDocument(doc.id).catch(() => {})}
            >
              <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
                <polyline points="15 3 21 3 21 9" />
                <line x1="10" y1="14" x2="21" y2="3" />
              </svg>
            </IconBtn>
            <IconBtn
              label={docFullscreen ? "Restore split layout" : "Hide chat (focus on document)"}
              onClick={() => setDocFullscreen(!docFullscreen)}
            >
              {docFullscreen ? (
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <rect x="3" y="3" width="18" height="18" rx="2" />
                  <line x1="12" y1="3" x2="12" y2="21" />
                </svg>
              ) : (
                <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <path d="M3 7V5a2 2 0 0 1 2-2h2" />
                  <path d="M17 3h2a2 2 0 0 1 2 2v2" />
                  <path d="M21 17v2a2 2 0 0 1-2 2h-2" />
                  <path d="M7 21H5a2 2 0 0 1-2-2v-2" />
                </svg>
              )}
            </IconBtn>
          </>
        )}
        <IconBtn
          label="Close viewer"
          onClick={() => setViewerDocumentId(null)}
        >
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </IconBtn>
      </header>

      <div className="flex-1 min-h-0 overflow-hidden bg-[#0a0b0e]">
        {error && (
          <div className="h-full flex items-center justify-center text-warn text-xs">
            {error}
          </div>
        )}

        {!error && doc && bodyKind === "pdf" && resolvedUrl && (
          <iframe
            key={resolvedUrl}
            src={resolvedUrl}
            title={doc.displayName}
            className="w-full h-full border-0 bg-white"
          />
        )}

        {!error && doc && bodyKind === "image" && resolvedUrl && (
          <div className="h-full w-full flex items-center justify-center p-4 overflow-auto">
            <img
              src={resolvedUrl}
              alt={doc.displayName}
              className="max-w-full max-h-full object-contain"
            />
          </div>
        )}

        {!error && doc && bodyKind === "text" && (
          <pre className="h-full w-full overflow-auto p-4 text-bone-2 text-[12px] font-mono whitespace-pre-wrap">
            {textBody ?? "Loading…"}
          </pre>
        )}

        {!error && doc && bodyKind === "unknown" && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className="h-full flex flex-col items-center justify-center gap-3 text-bone-3 text-xs px-6 text-center"
          >
            <DocumentIcon kind={doc.kind} mimeType={doc.mimeType} size={40} />
            <p>
              Preview not available for {doc.mimeType || doc.kind} inline.
            </p>
            <div className="flex gap-2">
              <button
                onClick={() => previewDocument(doc.id).catch(() => {})}
                className="px-3 py-1 rounded-md bg-pulse/[0.20] text-bone hover:bg-pulse/[0.30] transition-colors"
              >
                Open in OS viewer
              </button>
              <button
                onClick={() => downloadDocument(doc.id, doc.originalFilename).catch(() => {})}
                className="px-3 py-1 rounded-md bg-white/[0.06] text-bone-2 hover:bg-white/[0.10] transition-colors"
              >
                Download
              </button>
            </div>
          </motion.div>
        )}
      </div>
    </div>
  );
}

type BodyKind = "pdf" | "image" | "text" | "unknown";

function IconBtn({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="shrink-0 text-bone-3 hover:text-bone-2 hover:bg-white/[0.06] rounded p-1 transition-colors"
    >
      {children}
    </button>
  );
}
