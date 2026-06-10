import { useEffect, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import {
  downloadDocument,
  formatBytes,
  getDocument,
  getDocumentPath,
  previewDocument,
  revealDocumentInFolder,
  type Document,
} from "../lib/documents";
import { DocumentIcon } from "./DocumentIcon";
import { useAppStore } from "../stores/app";

interface Props {
  documentId: number;
}

/// Inline card for a generated or attached document. Shows icon, name,
/// size, kind, with quick-open via the OS default viewer. For image
/// MIME types we render an inline preview at the top of the card.
export function FileCard({ documentId }: Props) {
  const [doc, setDoc] = useState<Document | null>(null);
  const [imgSrc, setImgSrc] = useState<string | null>(null);
  const [opening, setOpening] = useState(false);
  const setViewerDocumentId = useAppStore((s) => s.setViewerDocumentId);

  useEffect(() => {
    getDocument(documentId)
      .then((d) => {
        setDoc(d);
        if (d && d.mimeType?.startsWith("image/")) {
          getDocumentPath(documentId)
            .then((p) => setImgSrc(convertFileSrc(p)))
            .catch(() => setImgSrc(null));
        }
      })
      .catch(() => setDoc(null));
  }, [documentId]);

  if (!doc) {
    return (
      <div className="text-bone-3 text-[11px] font-mono px-3 py-2 my-1.5">
        loading doc#{documentId}…
      </div>
    );
  }

  const isImage = doc.mimeType?.startsWith("image/");

  // v0.20.1 — primary click opens the in-Travis split-window viewer.
  // OS viewer stays available via the bottom-right icon button.
  const handleOpen = () => {
    setOpening(true);
    setViewerDocumentId(documentId);
    setTimeout(() => setOpening(false), 200);
  };

  const handleOpenExternal = async (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    try {
      await previewDocument(documentId);
    } catch {
      /* ignore */
    }
  };

  // v0.18.3 — stopPropagation so the surrounding button's "open file"
  // doesn't also fire when the user clicks the reveal icon.
  const handleReveal = async (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    try {
      await revealDocumentInFolder(documentId);
    } catch {
      /* ignore */
    }
  };

  // v0.20.1 — download button. Opens OS save dialog defaulted to the
  // original filename. stopPropagation so the surrounding open click
  // doesn't fire too.
  const handleDownload = async (e: React.MouseEvent) => {
    e.stopPropagation();
    e.preventDefault();
    if (!doc) return;
    try {
      await downloadDocument(documentId, doc.originalFilename);
    } catch {
      /* ignore */
    }
  };

  if (isImage && imgSrc) {
    return (
      <button
        onClick={handleOpen}
        disabled={opening}
        className="my-1.5 flex flex-col gap-1 rounded-md text-left max-w-[280px] overflow-hidden transition-colors hover:bg-pulse/10"
        style={{
          background: "rgba(124, 92, 255, 0.06)",
          border: "1px solid rgba(124, 92, 255, 0.20)",
        }}
        title="Click to open full-size"
      >
        <img
          src={imgSrc}
          alt={doc.displayName}
          className="max-h-[200px] w-full object-cover"
        />
        <div className="px-2.5 py-1.5">
          <div className="text-bone text-[12px] truncate">{doc.displayName}</div>
          <div className="text-bone-3 text-[10px] font-mono mt-0.5">
            {formatBytes(doc.sizeBytes)} · doc#{doc.id}
          </div>
        </div>
      </button>
    );
  }

  return (
    <button
      onClick={handleOpen}
      disabled={opening}
      className="my-1.5 flex items-center gap-3 px-3 py-2 rounded-md text-left w-full max-w-[420px] transition-colors hover:bg-pulse/10"
      style={{
        background: "rgba(124, 92, 255, 0.06)",
        border: "1px solid rgba(124, 92, 255, 0.20)",
      }}
      title="Open in default viewer"
    >
      <span className="shrink-0 text-bone-2">
        <DocumentIcon kind={doc.kind} mimeType={doc.mimeType} size={20} />
      </span>
      <div className="flex-1 min-w-0">
        <div className="text-bone text-[13px] truncate">{doc.displayName}</div>
        <div className="text-bone-3 text-[10px] font-mono mt-0.5">
          {doc.kind} · {formatBytes(doc.sizeBytes)} · doc#{doc.id}
        </div>
      </div>
      <span className="text-bone-3 text-[11px] shrink-0">
        {opening ? "opening…" : "preview"}
      </span>
      <IconAction label="Open in OS viewer" onClick={handleOpenExternal}>
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
          <polyline points="15 3 21 3 21 9" />
          <line x1="10" y1="14" x2="21" y2="3" />
        </svg>
      </IconAction>
      <IconAction
        label="Download"
        onClick={handleDownload}
      >
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" />
          <polyline points="7 10 12 15 17 10" />
          <line x1="12" y1="15" x2="12" y2="3" />
        </svg>
      </IconAction>
      <IconAction label="Show in file manager" onClick={handleReveal}>
        <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
          <path d="M22 19a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h5l2 3h9a2 2 0 0 1 2 2z" />
        </svg>
      </IconAction>
    </button>
  );
}

function IconAction({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: (e: React.MouseEvent) => void;
  children: React.ReactNode;
}) {
  return (
    <span
      role="button"
      tabIndex={0}
      onClick={onClick}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          onClick(e as unknown as React.MouseEvent);
        }
      }}
      className="text-bone-3 text-[11px] shrink-0 ml-1 px-1.5 py-0.5 rounded hover:bg-white/[0.08] hover:text-bone-2 cursor-pointer"
      title={label}
    >
      {children}
    </span>
  );
}
